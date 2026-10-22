//! One game window (plan S5, `docs/plans/2026-10-21-song-install-in-app.md`).
//! The first instance holds a named mutex and a message-only window. A later
//! launch (a `.bmsp` double-clicked in Explorer, or the game started again)
//! finds that window, sends it its path with `WM_COPYDATA` and exits; the
//! first instance comes to the front and adds the file through the drop
//! queue. No crate: user32 and kernel32 are called directly.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use winit::window::Window;

/// Tells a running instance about `path` (or just to come forward when
/// there is none). Returns true when one took it, so this process should exit.
/// Returns false when this is the first instance; it then owns the mutex.
pub fn hand_over(path: Option<&std::path::Path>) -> bool {
    imp::hand_over(path)
}

/// Starts the receiver for later launches. Paths they send arrive on the
/// returned channel, and `window` is brought forward and woken for each.
pub fn listen(window: Arc<Window>) -> Option<Receiver<PathBuf>> {
    imp::listen(window)
}

/// The `WM_COPYDATA` payload: the path as UTF-16, possibly empty.
pub fn encode(path: Option<&std::path::Path>) -> Vec<u16> {
    path.map(|p| p.to_string_lossy().encode_utf16().collect())
        .unwrap_or_default()
}

/// The path in a payload, or `None` for an empty one (a plain relaunch).
pub fn decode(data: &[u16]) -> Option<PathBuf> {
    (!data.is_empty()).then(|| PathBuf::from(String::from_utf16_lossy(data)))
}

#[cfg(windows)]
#[allow(non_snake_case, clippy::upper_case_acronyms)]
// raw_input.rs declares the same user32 calls with its own HWND and MSG types.
#[allow(clashing_extern_declarations)]
mod imp {
    use std::ffi::c_void;
    use std::path::{Path, PathBuf};
    use std::ptr;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, Mutex, OnceLock};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    const MUTEX_NAME: &str = "Local\\Beetle.SingleInstance";
    const CLASS_NAME: &str = "BeetleAppReceiver";
    const ERROR_ALREADY_EXISTS: u32 = 183;
    const WM_COPYDATA: u32 = 0x004A;
    const HWND_MESSAGE: isize = -3;
    const ASFW_ANY: u32 = u32::MAX;
    const SW_RESTORE: i32 = 9;
    const SMTO_ABORTIFHUNG: u32 = 0x2;
    /// Marks our payload, so a stray `WM_COPYDATA` is ignored.
    const TAG: usize = 0xBEE7_0001;

    #[repr(C)]
    struct CopyData {
        tag: usize,
        bytes: u32,
        data: *const c_void,
    }

    #[repr(C)]
    struct WndClass {
        size: u32,
        style: u32,
        proc_: unsafe extern "system" fn(isize, u32, usize, isize) -> isize,
        cls_extra: i32,
        wnd_extra: i32,
        instance: isize,
        icon: isize,
        cursor: isize,
        background: isize,
        menu_name: *const u16,
        class_name: *const u16,
        icon_sm: isize,
    }

    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        pt: [i32; 2],
        private: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(attrs: *const c_void, owner: i32, name: *const u16) -> isize;
        fn GetLastError() -> u32;
        fn GetModuleHandleW(name: *const u16) -> isize;
    }

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowExW(
            parent: isize,
            after: isize,
            class: *const u16,
            title: *const u16,
        ) -> isize;
        fn SendMessageTimeoutW(
            hwnd: isize,
            msg: u32,
            w: usize,
            l: isize,
            flags: u32,
            timeout: u32,
            result: *mut usize,
        ) -> isize;
        fn AllowSetForegroundWindow(pid: u32) -> i32;
        fn RegisterClassExW(class: *const WndClass) -> u16;
        fn CreateWindowExW(
            ex_style: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: isize,
            menu: isize,
            instance: isize,
            param: *const c_void,
        ) -> isize;
        fn DefWindowProcW(hwnd: isize, msg: u32, w: usize, l: isize) -> isize;
        fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> isize;
        fn SetForegroundWindow(hwnd: isize) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    pub fn hand_over(path: Option<&Path>) -> bool {
        let name = wide(MUTEX_NAME);
        // The handle is kept for the life of the process: it is the claim.
        let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
        if handle == 0 || unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
            return false;
        }
        let class = wide(CLASS_NAME);
        let payload = super::encode(path);
        // The first instance may still be starting: give its receiver a moment.
        for _ in 0..20 {
            let target = unsafe { FindWindowExW(HWND_MESSAGE, 0, class.as_ptr(), ptr::null()) };
            if target != 0 {
                let data = CopyData {
                    tag: TAG,
                    bytes: (payload.len() * 2) as u32,
                    data: payload.as_ptr() as *const c_void,
                };
                let mut result = 0usize;
                unsafe {
                    // Lets the first instance take the foreground we were given.
                    AllowSetForegroundWindow(ASFW_ANY);
                    SendMessageTimeoutW(
                        target,
                        WM_COPYDATA,
                        0,
                        &data as *const CopyData as isize,
                        SMTO_ABORTIFHUNG,
                        5000,
                        &mut result,
                    );
                }
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // No receiver answered: run as a second window rather than lose the file.
        false
    }

    struct Listener {
        tx: Mutex<Sender<PathBuf>>,
        window: Arc<Window>,
        hwnd: isize,
    }

    static LISTENER: OnceLock<Listener> = OnceLock::new();

    unsafe extern "system" fn proc_(hwnd: isize, msg: u32, w: usize, l: isize) -> isize {
        if msg == WM_COPYDATA && l != 0 {
            let data = &*(l as *const CopyData);
            if data.tag == TAG {
                if let Some(listener) = LISTENER.get() {
                    let len = data.bytes as usize / 2;
                    let units = if len == 0 || data.data.is_null() {
                        &[][..]
                    } else {
                        std::slice::from_raw_parts(data.data as *const u16, len)
                    };
                    if let Some(path) = super::decode(units) {
                        if let Ok(tx) = listener.tx.lock() {
                            let _ = tx.send(path);
                        }
                    }
                    if IsIconic(listener.hwnd) != 0 {
                        ShowWindow(listener.hwnd, SW_RESTORE);
                    }
                    SetForegroundWindow(listener.hwnd);
                    listener.window.request_redraw();
                }
                return 1;
            }
        }
        DefWindowProcW(hwnd, msg, w, l)
    }

    pub fn listen(window: Arc<Window>) -> Option<Receiver<PathBuf>> {
        let hwnd = match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(win32) => win32.hwnd.get(),
            _ => return None,
        };
        let (tx, rx) = channel();
        LISTENER
            .set(Listener {
                tx: Mutex::new(tx),
                window,
                hwnd,
            })
            .ok()?;
        std::thread::spawn(|| unsafe {
            let class = wide(CLASS_NAME);
            let instance = GetModuleHandleW(ptr::null());
            let wc = WndClass {
                size: std::mem::size_of::<WndClass>() as u32,
                style: 0,
                proc_,
                cls_extra: 0,
                wnd_extra: 0,
                instance,
                icon: 0,
                cursor: 0,
                background: 0,
                menu_name: ptr::null(),
                class_name: class.as_ptr(),
                icon_sm: 0,
            };
            RegisterClassExW(&wc);
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                0,
                instance,
                ptr::null(),
            );
            if hwnd == 0 {
                return;
            }
            let mut msg: Msg = std::mem::zeroed();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                DispatchMessageW(&msg);
            }
        });
        Some(rx)
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::Receiver;
    use std::sync::Arc;
    use winit::window::Window;

    pub fn hand_over(_path: Option<&Path>) -> bool {
        false
    }

    pub fn listen(_window: Arc<Window>) -> Option<Receiver<PathBuf>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_round_trips_and_empty_means_none() {
        let p = std::path::Path::new(r"D:\곡\패키지.bmsp");
        assert_eq!(decode(&encode(Some(p))).as_deref(), Some(p));
        assert_eq!(decode(&encode(None)), None);
    }
}
