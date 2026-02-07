use jni::objects::{JClass, JObject};
use jni::sys::{jint, jobject};
use jni::JNIEnv;

use ndk::native_window::NativeWindow;
use std::ptr::NonNull;
use std::os::raw::c_void;
use std::thread;

mod input;

/// Starts the OpenGL renderer on a separate thread
pub fn start_renderer(window: Option<NativeWindow>, width: i32, height: i32, xdpi: f32, ydpi: f32, fps: i32) {
    if let Some(window) = window {
        // Wrap pointer in NonNull for thread safety
        let window_ptr = NonNull::new(window.ptr().as_ptr() as *mut c_void)
            .expect("Failed to get NonNull pointer from NativeWindow");

        thread::spawn(move || {
            unsafe {
                // Call the FFI renderer
                renderer_bindings::startOpenGLRenderer(
                    window_ptr.as_ptr() as *mut c_void,
                    width,
                    height,
                    xdpi as i32,
                    ydpi as i32,
                    fps as i32,
                );
            }
        });
    } else {
        panic!("NativeWindow was None!");
    }
}

/// JNI function to reset or set a new window for the renderer
#[no_mangle]
pub extern "system" fn renderer_reset_window(
    env: JNIEnv,
    _class: JClass,
    surface: jobject,
    _top: jint,
    _left: jint,
    _width: jint,
    _height: jint,
    xdpi: jint,
    ydpi: jint,
    fps: jint,
) {
    // Convert Java surface into NativeWindow
    let window = unsafe { NativeWindow::from_surface(&env, JObject::from(surface)) };
    
    start_renderer(window, _width, _height, xdpi as f32, ydpi as f32, fps as i32);
}

/// JNI function to handle touch events forwarded from Java
#[no_mangle]
pub extern "system" fn handle_motion_event(env: JNIEnv, _class: JClass, motion_event: jobject) {
    // Convert the Java MotionEvent into ndk::MotionEvent
    if let Ok(ev) = ndk::event::MotionEvent::from_java(&env, motion_event) {
        input::handle_touch(ev);
    }
}

/// Starts the input server (Unix socket) for multi-touch injection
#[no_mangle]
pub extern "system" fn start_input(env: JNIEnv, _class: JClass, width: jint, height: jint) {
    input::start_input_system(width, height);
}

/// Optionally send a key code event
#[no_mangle]
pub extern "system" fn send_key(env: JNIEnv, _class: JClass, code: jint) {
    input::send_key_code(code);
}
