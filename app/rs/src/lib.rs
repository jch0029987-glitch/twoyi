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
    let window_ptr = unsafe { ndk_sys::ANativeWindow_fromSurface(env.get_native_interface(), surface) };
    let nonnull_ptr = match std::ptr::NonNull::new(window_ptr) {
        Some(p) => p,
        None => { return; }
    };

    let window = unsafe { ndk::native_window::NativeWindow::from_ptr(nonnull_ptr) };
    let width = window.width();
    let height = window.height();

    if RENDERER_STARTED.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
        input::start_input_system(width, height);
        thread::spawn(move || {
            unsafe { renderer_bindings::startOpenGLRenderer(window_ptr as *mut c_void, width, height, xdpi as i32, ydpi as i32, fps as i32); }
        });

        let loader_jstr = unsafe { JString::from_raw(loader) };
        if let Ok(l_path) = env.get_string(&loader_jstr) {
            let working_dir = "/data/data/io.twoyi/rootfs";
            let _ = Command::new("./init")
                .current_dir(working_dir)
                .env("TYLOADER", String::from(l_path))
                .spawn();
        }
    } else {
        unsafe {
            renderer_bindings::setNativeWindow(window_ptr as *mut c_void);
            renderer_bindings::resetSubWindow(window_ptr as *mut c_void, 0, 0, width, height, width, height, 1.0, 0.0);
        }
    }
}

#[no_mangle]
pub extern "system" fn handle_touch(mut env: JNIEnv, _clz: jclass, event: jobject) {
    let obj = unsafe { JObject::from_raw(event) };
    if let Ok(JValue::Long(p)) = env.get_field(&obj, "mNativePtr", "J") {
        let ev_ptr = p as *mut ndk_sys::AInputEvent;
        if let Some(nonptr) = std::ptr::NonNull::new(ev_ptr) {
            let ev = unsafe { ndk::event::MotionEvent::from_ptr(nonptr) };
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
    android_logger::init_once(Config::default().with_max_level(LevelFilter::Info).with_tag("CLIENT_EGL"));
    let mut env = jvm.get_env().unwrap();
    let jni_methods = [
        jni_method!(init, renderer_init, "(Landroid/view/Surface;Ljava/lang/String;FFI)V"),
        jni_method!(handleTouch, handle_touch, "(Landroid/view/MotionEvent;)V"),
        jni_method!(sendKeycode, send_key_code, "(I)V"),
    ];
    let clazz = env.find_class("io/twoyi/Renderer").unwrap();
    let _ = env.register_native_methods(&clazz, &jni_methods);
    jni::sys::JNI_VERSION_1_6
}
