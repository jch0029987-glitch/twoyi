use jni::objects::{JValueOwned, JObject, JString};
use jni::sys::{jclass, jfloat, jint, jobject, jstring};
use jni::JNIEnv;
use jni::{JavaVM, NativeMethod};
use log::{error, info, LevelFilter};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use android_logger::Config;
use std::fs::File;
use std::process::{Command, Stdio};

mod input;
mod renderer_bindings;

use ndk::native_window::NativeWindow;

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
    let window = unsafe { NativeWindow::from_surface(env.get_native_interface(), surface) }
        .expect("Failed to get NativeWindow from surface");
    let width = window.width();
    let height = window.height();

    let window_ptr = window.ptr().as_ptr() as *mut c_void;

    if RENDERER_STARTED.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
        info!("Initializing Twoyi Renderer: {}x{}", width, height);
        input::start_input_system(width, height);

        thread::spawn(move || {
            unsafe {
                renderer_bindings::startOpenGLRenderer(window_ptr, width, height, xdpi as i32, ydpi as i32, fps as i32);
            }
        });

        let loader_jstr = unsafe { JString::from_raw(loader) };
        if let Ok(l_path) = env.get_string(&loader_jstr) {
            let loader_path: String = l_path.into();
            let log_path = "/data/data/io.twoyi/log.txt";
            if let Ok(outputs) = File::create(log_path) {
                let errors = outputs.try_clone().unwrap();
                let _ = Command::new("./init")
                    .current_dir("/data/data/io.twoyi/rootfs")
                    .env("TYLOADER", loader_path)
                    .stdout(Stdio::from(outputs))
                    .stderr(Stdio::from(errors))
                    .spawn();
            }
        }
    } else {
        unsafe {
            renderer_bindings::setNativeWindow(window_ptr);
            renderer_bindings::resetSubWindow(window_ptr, 0, 0, width, height, width, height, 1.0, 0.0);
        }
    }
}

#[no_mangle]
pub extern "system" fn renderer_reset_window(mut env: JNIEnv, _clz: jclass, surface: jobject, _top: jint, _left: jint, _width: jint, _height: jint) {
    let window = unsafe { NativeWindow::from_surface(env.get_native_interface(), surface) }
        .expect("Failed to get NativeWindow from surface");
    let window_ptr = window.ptr().as_ptr() as *mut c_void;
    unsafe {
        renderer_bindings::resetSubWindow(window_ptr, 0, 0, _width, _height, _width, _height, 1.0, 0.0);
    }
}

#[no_mangle]
pub extern "system" fn renderer_remove_window(mut env: JNIEnv, _clz: jclass, surface: jobject) {
    let window = unsafe { NativeWindow::from_surface(env.get_native_interface(), surface) }
        .expect("Failed to get NativeWindow from surface");
    let window_ptr = window.ptr().as_ptr() as *mut c_void;
    unsafe {
        renderer_bindings::removeSubWindow(window_ptr);
    }
}

#[no_mangle]
pub extern "system" fn handle_touch(mut env: JNIEnv, _clz: jclass, event: jobject) {
    let obj = unsafe { JObject::from_raw(event) };

    if let Ok(JValueOwned::Long(p)) = env.get_field(&obj, "mNativePtr", "J") {
        let ev_ptr = p as *mut c_void;
        if let Some(nonptr) = std::ptr::NonNull::new(ev_ptr) {
            let ev = unsafe { ndk::event::MotionEvent::from_ptr(nonptr.cast()) };
            input::handle_touch(ev);
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
            .with_tag("CLIENT_EGL")
    );

    let mut env = jvm.get_env().unwrap();
    let jni_methods = [
        jni_method!(init, renderer_init, "(Landroid/view/Surface;Ljava/lang/String;FFI)V"),
        jni_method!(resetWindow, renderer_reset_window, "(Landroid/view/Surface;IIII)V"),
        jni_method!(removeWindow, renderer_remove_window, "(Landroid/view/Surface;)V"),
        jni_method!(handleTouch, handle_touch, "(Landroid/view/MotionEvent;)V"),
        jni_method!(sendKeycode, send_key_code, "(I)V"),
    ];

    let clazz = env.find_class("io/twoyi/Renderer").unwrap();
    let _ = env.register_native_methods(&clazz, &jni_methods);

    jni::sys::JNI_VERSION_1_6
}
