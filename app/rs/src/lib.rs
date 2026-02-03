/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use jni::objects::{JValue, JObject, JString};
use jni::sys::{jclass, jfloat, jint, jobject, jstring};
use jni::JNIEnv;
use jni::{JavaVM, NativeMethod};
use log::{error, info, debug, LevelFilter};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use android_logger::Config;
use std::fs::File;
use std::process::{Command, Stdio};

mod input;
mod renderer_bindings;

macro_rules! jni_method {
    ( $name: tt, $method:tt, $signature:expr ) => {{
        jni::NativeMethod {
            name: jni::strings::JNIString::from(stringify!($name)),
            sig: jni::strings::JNIString::from($signature),
            fn_ptr: $method as *mut c_void,
        }
    }};
}

static RENDERER_STARTED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub extern "system" fn renderer_init(
    mut env: JNIEnv,
    _clz: jclass,
    surface: jobject,
    loader: jstring,
    xdpi: jfloat,
    ydpi: jfloat,
    fps: jint,
) {
    debug!("renderer_init");
    let window_ptr = unsafe { ndk_sys::ANativeWindow_fromSurface(env.get_native_interface(), surface) };

    let nonnull_ptr = match std::ptr::NonNull::new(window_ptr) {
        Some(p) => p,
        None => {
            error!("ANativeWindow_fromSurface was null!");
            return;
        }
    };

    // Correct for ndk 0.8.0
    let window = unsafe { ndk::native_window::NativeWindow::from_ptr(nonnull_ptr) };
    let width = window.width();
    let height = window.height();

    info!("renderer_init width: {}, height: {}, fps: {}", width, height, fps);

    if RENDERER_STARTED.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
        input::start_input_system(width, height);

        thread::spawn(move || {
            let win = window_ptr as *mut c_void;
            unsafe {
                renderer_bindings::startOpenGLRenderer(win, width, height, xdpi as i32, ydpi as i32, fps as i32);
            }
        });

        let loader_jstr = unsafe { JString::from_raw(loader) };
        let loader_path: String = env.get_string(&loader_jstr).unwrap().into();
        let working_dir = "/data/data/io.twoyi/rootfs";
        let log_path = "/data/data/io.twoyi/log.txt";
        
        if let Ok(outputs) = File::create(log_path) {
            let errors = outputs.try_clone().unwrap();
            let _ = Command::new("./init")
                .current_dir(working_dir)
                .env("TYLOADER", loader_path)
                .stdout(Stdio::from(outputs))
                .stderr(Stdio::from(errors))
                .spawn();
        }
    } else {
        let win = window_ptr as *mut c_void;
        unsafe {
            renderer_bindings::setNativeWindow(win);
            renderer_bindings::resetSubWindow(win, 0, 0, width, height, width, height, 1.0, 0.0);
        }
    }
}

#[no_mangle]
pub extern "system" fn renderer_reset_window(mut env: JNIEnv, _clz: jclass, surface: jobject, _top: jint, _left: jint, _width: jint, _height: jint) {
    unsafe {
        let window = ndk_sys::ANativeWindow_fromSurface(env.get_native_interface(), surface);
        renderer_bindings::resetSubWindow(window as *mut c_void, 0, 0, _width, _height, _width, _height, 1.0, 0.0);
    }
}

#[no_mangle]
pub extern "system" fn renderer_remove_window(mut env: JNIEnv, _clz: jclass, surface: jobject) {
    unsafe {
        let window = ndk_sys::ANativeWindow_fromSurface(env.get_native_interface(), surface);
        renderer_bindings::removeSubWindow(window as *mut c_void);
    }
}

#[no_mangle]
pub extern "system" fn handle_touch(mut env: JNIEnv, _clz: jclass, event: jobject) {
    let obj = unsafe { JObject::from_raw(event) };
    if let Ok(ptr) = env.get_field(&obj, "mNativePtr", "J") {
        if let JValue::Long(p) = &ptr { 
            let ev_ptr = unsafe { std::mem::transmute::<i64, *mut ndk_sys::AInputEvent>(*p) };
            if let Some(nonptr) = std::ptr::NonNull::new(ev_ptr) {
                let ev = unsafe { ndk::event::MotionEvent::from_ptr(nonptr) };
                input::handle_touch(ev);
            }
        }
    }
}

#[no_mangle]
pub extern "system" fn send_key_code(_env: JNIEnv, _clz: jclass, keycode: jint) {
    input::send_key_code(keycode);
}

#[no_mangle]
#[allow(non_snake_case)]
unsafe fn JNI_OnLoad(jvm: JavaVM, _reserved: *mut c_void) -> jint {
    android_logger::init_once(
        Config::default()
            .with_max_level(LevelFilter::Info)
            .with_tag("CLIENT_EGL"),
    );

    let mut env = jvm.get_env().unwrap();
    let class_name = "io/twoyi/Renderer";
    let jni_methods = [
        jni_method!(init, renderer_init, "(Landroid/view/Surface;Ljava/lang/String;FFI)V"),
        jni_method!(resetWindow, renderer_reset_window, "(Landroid/view/Surface;IIII)V"),
        jni_method!(removeWindow, renderer_remove_window, "(Landroid/view/Surface;)V"),
        jni_method!(handleTouch, handle_touch, "(Landroid/view/MotionEvent;)V"),
        jni_method!(sendKeycode, send_key_code, "(I)V"),
    ];

    let clazz = env.find_class(class_name).unwrap();
    let _ = env.register_native_methods(&clazz, &jni_methods);
    
    jni::sys::JNI_VERSION_1_6
}
