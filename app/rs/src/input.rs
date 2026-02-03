use libc::*;
use ndk::event::{MotionAction, MotionEvent};
use std::thread;
use std::io::Write;
use unix_socket::UnixListener;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use once_cell::sync::Lazy;

const TOUCH_PATH: &'static str = "/data/data/io.twoyi/rootfs/dev/input/touch";
static INPUT_SENDER: Lazy<Mutex<Option<Sender<input_event>>>> = Lazy::new(|| Mutex::new(None));

pub fn start_input_system(width: i32, height: i32) {
    thread::spawn(move || { touch_server(width, height); });
}

pub fn input_event_write(tx: &Sender<input_event>, kind: i32, code: i32, val: i32) {
    let mut tp = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { clock_gettime(CLOCK_MONOTONIC, &mut tp); }
    let ev = input_event {
        type_: kind as u16,
        code: code as u16,
        value: val,
        time: timeval { tv_sec: tp.tv_sec, tv_usec: (tp.tv_nsec / 1000) as suseconds_t },
    };
    let _ = tx.send(ev);
}

pub fn handle_touch(ev: MotionEvent) {
    if let Some(ref tx) = *INPUT_SENDER.lock().unwrap() {
        let action = ev.action();
        let pointer = ev.pointer_at_index(ev.pointer_index());
        
        input_event_write(tx, EV_ABS, ABS_MT_POSITION_X, pointer.x() as i32);
        input_event_write(tx, EV_ABS, ABS_MT_POSITION_Y, pointer.y() as i32);
        input_event_write(tx, EV_SYN, SYN_REPORT, 0);
    }
}

fn touch_server(_w: i32, _h: i32) {
    let _ = std::fs::remove_file(TOUCH_PATH);
    if let Ok(listener) = UnixListener::bind(TOUCH_PATH) {
        for stream in listener.incoming() {
            if let Ok(mut s) = stream {
                let (tx, rx) = channel();
                *INPUT_SENDER.lock().unwrap() = Some(tx);
                while let Ok(ev) = rx.recv() {
                    let data = unsafe { std::slice::from_raw_parts(&ev as *const _ as *const u8, std::mem::size_of::<input_event>()) };
                    if s.write_all(data).is_err() { break; }
                }
            }
        }
    }
}

pub fn send_key_code(_k: i32) {}
