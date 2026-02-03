/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use libc::*;
use ndk::event::{MotionAction, MotionEvent};
use std::mem;
use std::thread;
use std::io::Write;

use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use once_cell::sync::Lazy;

use log::info;

// Replacement constants for uinput_sys/input.h
const FF_MAX: u16 = 0x7f;
const ABS_CNT: usize = 0x40;
const KEY_MAX: u16 = 0x2ff;
const ABS_MAX: u16 = 0x3f;
const REL_MAX: u16 = 0x0f;
const SW_MAX: u16 = 0x10;
const LED_MAX: u16 = 0x0f;
const INPUT_PROP_MAX: u16 = 0x1f;

const TOUCH_PATH: &'static str = "/data/data/io.twoyi/rootfs/dev/input/touch";
const TOUCH_DEVICE_NAME: &'static str = "vtouch";
const TOUCH_DEVICE_UNIQUE_ID: &'static str = "<vtouch 0>";

const KEY_DEVICE_NAME: &'static str = "vkey";
const KEY_DEVICE_UNIQUE_ID: &'static str = "<keyboard 0>";
const KEY_PATH: &'static str = "/data/data/io.twoyi/rootfs/dev/input/key0";

#[repr(C)]
#[derive(Clone, Copy)]
pub struct device_info {
    pub name: [c_char; 80],
    pub driver_version: c_int,
    pub id: input_id,
    pub physical_location: [c_char; 80],
    pub unique_id: [c_char; 80],
    pub key_bitmask: [u8; (KEY_MAX as usize + 1) / 8],
    pub abs_bitmask: [u8; (ABS_MAX as usize + 1) / 8],
    pub rel_bitmask: [u8; (REL_MAX as usize + 1) / 8],
    pub sw_bitmask: [u8; (SW_MAX as usize + 1) / 8],
    pub led_bitmask: [u8; (LED_MAX as usize + 1) / 8],
    pub ff_bitmask: [u8; (FF_MAX as usize + 1) / 8],
    pub prop_bitmask: [u8; (INPUT_PROP_MAX as usize + 1) / 8],
    pub abs_max: [u32; ABS_CNT],
    pub abs_min: [u32; ABS_CNT],
}

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
}

fn copy_to_cstr<const COUNT: usize>(data: &str, arr: &mut [i8; COUNT]) {
    let cstr = std::ffi::CString::new(data).expect("create cstring failed");
    let bytes = cstr.as_bytes_with_nul();
    let mut len = bytes.len();
    if len >= COUNT {
        len = COUNT;
    }
    // Convert to i8 for C-style char arrays
    for i in 0..len {
        arr[i] = bytes[i] as i8;
    }
}

const MAX_POINTERS: usize = 5;

static INPUT_SENDER: Lazy<Mutex<Option<Sender<input_event>>>> = Lazy::new(|| { Mutex::new(None)});
static KEY_SENDER: Lazy<Mutex<Option<Sender<input_event>>>> = Lazy::new(|| { Mutex::new(None)});

pub fn start_input_system(width: i32, height: i32) {
    thread::spawn(move || {
        touch_server(width, height);
    });
    thread::spawn(|| {
        key_server();
    });
}

pub fn input_event_write(
    tx: &std::sync::mpsc::Sender<input_event>,
    kind: i32,
    code: i32,
    val: i32,
) {
    let mut tp = libc::timespec { tv_sec:0, tv_nsec: 0 };
    let _ = unsafe { clock_gettime(CLOCK_MONOTONIC, &mut tp) };
    let tv = timeval {
        tv_sec: tp.tv_sec,
        tv_usec: (tp.tv_nsec / 1000) as suseconds_t
    };

    let ev = input_event {
        type_: kind as u16, // libc uses type_
        code: code as u16,
        value: val,
        time: tv,
    };
    let _ = tx.send(ev);
}

pub fn handle_touch(ev: MotionEvent) {
    let opt = INPUT_SENDER.lock().unwrap();
    if let Some(ref fd) = *opt {
        let action = ev.action();
        let pointer_index = ev.pointer_index();
        let pointer = ev.pointer_at_index(pointer_index);
        let pointer_id = pointer.pointer_id();
        let pressure = pointer.pressure();

        static G_INPUT_MT: Lazy<Mutex<[i32;MAX_POINTERS]>> = Lazy::new(|| {std::sync::Mutex::new([0i32;MAX_POINTERS])});

        match action {
            MotionAction::Down | MotionAction::PointerDown => {
                let x = pointer.x();
                let y = pointer.y();

                let mut mt = G_INPUT_MT.lock().unwrap();
                mt[pointer_id as usize] = 1;

                input_event_write(fd, EV_ABS, ABS_MT_SLOT, pointer_id);
                input_event_write(fd, EV_ABS, ABS_MT_TRACKING_ID, pointer_id + 1);

                if pointer_id == 0 {
                    input_event_write(fd, EV_KEY, BTN_TOUCH, 1);
                    input_event_write(fd, EV_KEY, BTN_TOOL_FINGER, 1);
                }

                input_event_write(fd, EV_ABS, ABS_MT_POSITION_X, x as i32);
                input_event_write(fd, EV_ABS, ABS_MT_POSITION_Y, y as i32);
                input_event_write(fd, EV_ABS, ABS_MT_PRESSURE, pressure as i32);
                input_event_write(fd, EV_SYN, SYN_REPORT, 0);
            }
            MotionAction::Up | MotionAction::PointerUp | MotionAction::Cancel => {
                let mut mt = G_INPUT_MT.lock().unwrap();
                if mt[pointer_id as usize] != 0 {
                    mt[pointer_id as usize] = 0;
                    input_event_write(fd, EV_ABS, ABS_MT_SLOT, pointer_id);
                    input_event_write(fd, EV_ABS, ABS_MT_TRACKING_ID, -1);
                    
                    if mt.iter().all(|&x| x == 0) {
                        input_event_write(fd, EV_KEY, BTN_TOUCH, 0);
                        input_event_write(fd, EV_KEY, BTN_TOOL_FINGER, 0);
                    }
                    input_event_write(fd, EV_SYN, SYN_REPORT, 0);
                }
            }
            MotionAction::Move => {
                let mt = G_INPUT_MT.lock().unwrap();
                if mt[pointer_id as usize] != 0 {
                    let x = pointer.x();
                    let y = pointer.y();
                    input_event_write(fd, EV_ABS, ABS_MT_SLOT, pointer_id);
                    input_event_write(fd, EV_ABS, ABS_MT_POSITION_X, x as i32);
                    input_event_write(fd, EV_ABS, ABS_MT_POSITION_Y, y as i32);
                    input_event_write(fd, EV_ABS, ABS_MT_PRESSURE, pressure as i32);
                    input_event_write(fd, EV_SYN, SYN_REPORT, 0);
                }
            }
            _ => {}
        }
    }
}

fn generate_touch_device(width: i32, height: i32) -> device_info {
    let mut info: device_info = unsafe { mem::zeroed() };
    
    info.driver_version = 0x1;
    info.id.bustype = 0x0006; // BUS_VIRTUAL
    
    copy_to_cstr(TOUCH_DEVICE_NAME, &mut info.name);
    copy_to_cstr(TOUCH_PATH, &mut info.physical_location);
    copy_to_cstr(TOUCH_DEVICE_UNIQUE_ID, &mut info.unique_id);

    info.prop_bitmask[0] = 1; // INPUT_PROP_DIRECT
    info.abs_bitmask[(ABS_MT_POSITION_X / 8) as usize] |= 1 << (ABS_MT_POSITION_X % 8);
    info.abs_bitmask[(ABS_MT_POSITION_Y / 8) as usize] |= 1 << (ABS_MT_POSITION_Y % 8);

    info.abs_max[ABS_MT_POSITION_X as usize] = width as u32;
    info.abs_max[ABS_MT_POSITION_Y as usize] = height as u32;
    info.abs_max[ABS_MT_SLOT as usize] = (MAX_POINTERS - 1) as u32;
    info.abs_max[ABS_MT_TRACKING_ID as usize] = 65535;

    info
}

fn touch_server(width: i32, height: i32) {
    let device = generate_touch_device(width, height);
    let _ = std::fs::remove_file(TOUCH_PATH);
    let listener = unix_socket::UnixListener::bind(TOUCH_PATH).unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                info!("touch client connected!");
                let data: &[u8] = unsafe { any_as_u8_slice(&device) };
                let _ = stream.write_all(data);

                let (tx, rx) = channel::<input_event>();
                *INPUT_SENDER.lock().unwrap() = Some(tx);

                thread::spawn(move || loop {
                    if let Ok(ev) = rx.recv() {
                        let ev_data: &[u8] = unsafe { any_as_u8_slice(&ev) };
                        if stream.write_all(ev_data).is_err() { break; }
                    }
                });
            }
            Err(_) => break,
        }
    }
}

fn generate_key_device() -> device_info {
    let mut info: device_info = unsafe { mem::zeroed() };
    info.driver_version = 0x1;
    copy_to_cstr(KEY_DEVICE_NAME, &mut info.name);
    copy_to_cstr(KEY_PATH, &mut info.physical_location);
    copy_to_cstr(KEY_DEVICE_UNIQUE_ID, &mut info.unique_id);
    info.key_bitmask[(KEY_BACK / 8) as usize] |= 1 << (KEY_BACK % 8);
    info
}

pub fn send_key_code(_keycode: i32) {
    if let Some(ref tx) = *KEY_SENDER.lock().unwrap() {
        input_event_write(tx, EV_KEY, KEY_BACK as i32, 1);
        input_event_write(tx, EV_SYN, SYN_REPORT, 0);
        input_event_write(tx, EV_KEY, KEY_BACK as i32, 0);
        input_event_write(tx, EV_SYN, SYN_REPORT, 0);
    }
}

fn key_server() {
    let device = generate_key_device();
    let _ = std::fs::remove_file(KEY_PATH);
    let listener = unix_socket::UnixListener::bind(KEY_PATH).unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                info!("key client connected!");
                let data: &[u8] = unsafe { any_as_u8_slice(&device) };
                let _ = stream.write_all(data);

                let (tx, rx) = channel::<input_event>();
                *KEY_SENDER.lock().unwrap() = Some(tx);

                thread::spawn(move || loop {
                    if let Ok(ev) = rx.recv() {
                        let ev_data: &[u8] = unsafe { any_as_u8_slice(&ev) };
                        if stream.write_all(ev_data).is_err() { break; }
                    }
                });
            }
            Err(_) => break,
        }
    }
}
