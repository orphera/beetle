//! The Windows "Open" dialog (`IFileOpenDialog`) through direct COM calls, so
//! the user picks folders and files instead of typing their paths. No crate:
//! the few vtable slots it needs are called by index, as `clipboard.rs` calls
//! user32.
//!
//! The dialog runs on its own thread with its own COM apartment. It is owned by
//! the main window, so Windows keeps the main window disabled while it is open,
//! but the main window still repaints and shows background progress.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

/// What the dialog picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickKind {
    Folder,
    /// A file, offered with one named filter such as ("BMS package", "*.bmsp").
    File {
        filter_name: &'static str,
        filter_spec: &'static str,
    },
}

/// Opens the dialog over the window `owner` (an HWND, 0 for none) and returns
/// at once; the receiver gets the picked path, or `None` when the user cancels.
pub fn pick(kind: PickKind, title: &str, owner: isize) -> Receiver<Option<PathBuf>> {
    let (tx, rx) = channel();
    let title = title.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(imp::pick(kind, &title, owner));
    });
    rx
}

#[cfg(target_os = "windows")]
#[allow(non_snake_case, clippy::upper_case_acronyms)]
mod imp {
    use super::PickKind;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use std::ptr;

    #[repr(C)]
    struct GUID {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[repr(C)]
    struct FilterSpec {
        name: *const u16,
        spec: *const u16,
    }

    const CLSID_FILE_OPEN_DIALOG: GUID = GUID {
        data1: 0xDC1C_5A9C,
        data2: 0xE88A,
        data3: 0x4DDE,
        data4: [0xA5, 0xA1, 0x60, 0xF8, 0x2A, 0x20, 0xAE, 0xF7],
    };
    const IID_IFILE_OPEN_DIALOG: GUID = GUID {
        data1: 0xD57C_7288,
        data2: 0xD4AD,
        data3: 0x4768,
        data4: [0xBE, 0x02, 0x9D, 0x96, 0x95, 0x32, 0xD9, 0x60],
    };

    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const COINIT_DISABLE_OLE1DDE: u32 = 0x4;
    const CLSCTX_INPROC_SERVER: u32 = 0x1;
    const FOS_PICKFOLDERS: u32 = 0x20;
    const FOS_FORCEFILESYSTEM: u32 = 0x40;
    const FOS_PATHMUSTEXIST: u32 = 0x800;
    const FOS_FILEMUSTEXIST: u32 = 0x1000;
    const SIGDN_FILESYSPATH: u32 = 0x8005_8000;

    // Vtable slots: IUnknown (0-2), IModalWindow::Show (3), then IFileDialog.
    const RELEASE: usize = 2;
    const SHOW: usize = 3;
    const SET_FILE_TYPES: usize = 4;
    const SET_OPTIONS: usize = 9;
    const GET_OPTIONS: usize = 10;
    const SET_TITLE: usize = 17;
    const GET_RESULT: usize = 20;
    // IShellItem::GetDisplayName.
    const GET_DISPLAY_NAME: usize = 5;

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> i32;
        fn CoUninitialize();
        fn CoCreateInstance(
            clsid: *const GUID,
            outer: *mut c_void,
            context: u32,
            iid: *const GUID,
            out: *mut *mut c_void,
        ) -> i32;
        fn CoTaskMemFree(p: *mut c_void);
    }

    /// The function in slot `index` of the COM object `obj`'s vtable.
    unsafe fn slot(obj: *mut c_void, index: usize) -> *const c_void {
        let vtable = *(obj as *const *const *const c_void);
        *vtable.add(index)
    }

    unsafe fn release(obj: *mut c_void) {
        let f: extern "system" fn(*mut c_void) -> u32 = std::mem::transmute(slot(obj, RELEASE));
        f(obj);
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn pick(kind: PickKind, title: &str, owner: isize) -> Option<PathBuf> {
        unsafe {
            let init = CoInitializeEx(
                ptr::null_mut(),
                COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE,
            );
            let result = show(kind, title, owner);
            if init >= 0 {
                CoUninitialize();
            }
            result
        }
    }

    unsafe fn show(kind: PickKind, title: &str, owner: isize) -> Option<PathBuf> {
        let mut dialog: *mut c_void = ptr::null_mut();
        if CoCreateInstance(
            &CLSID_FILE_OPEN_DIALOG,
            ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_IFILE_OPEN_DIALOG,
            &mut dialog,
        ) < 0
            || dialog.is_null()
        {
            return None;
        }

        let get_options: extern "system" fn(*mut c_void, *mut u32) -> i32 =
            std::mem::transmute(slot(dialog, GET_OPTIONS));
        let set_options: extern "system" fn(*mut c_void, u32) -> i32 =
            std::mem::transmute(slot(dialog, SET_OPTIONS));
        let mut options = 0u32;
        get_options(dialog, &mut options);
        options |= FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST;
        options |= match kind {
            PickKind::Folder => FOS_PICKFOLDERS,
            PickKind::File { .. } => FOS_FILEMUSTEXIST,
        };
        set_options(dialog, options);

        let title_w = wide(title);
        let set_title: extern "system" fn(*mut c_void, *const u16) -> i32 =
            std::mem::transmute(slot(dialog, SET_TITLE));
        set_title(dialog, title_w.as_ptr());

        // The filter strings must outlive Show.
        let filter_strings;
        if let PickKind::File {
            filter_name,
            filter_spec,
        } = kind
        {
            filter_strings = [
                wide(filter_name),
                wide(filter_spec),
                wide("All files"),
                wide("*.*"),
            ];
            let specs = [
                FilterSpec {
                    name: filter_strings[0].as_ptr(),
                    spec: filter_strings[1].as_ptr(),
                },
                FilterSpec {
                    name: filter_strings[2].as_ptr(),
                    spec: filter_strings[3].as_ptr(),
                },
            ];
            let set_file_types: extern "system" fn(*mut c_void, u32, *const FilterSpec) -> i32 =
                std::mem::transmute(slot(dialog, SET_FILE_TYPES));
            set_file_types(dialog, specs.len() as u32, specs.as_ptr());
        }

        let show: extern "system" fn(*mut c_void, isize) -> i32 =
            std::mem::transmute(slot(dialog, SHOW));
        if show(dialog, owner) < 0 {
            // Cancelled (or failed): nothing picked.
            release(dialog);
            return None;
        }

        let mut item: *mut c_void = ptr::null_mut();
        let get_result: extern "system" fn(*mut c_void, *mut *mut c_void) -> i32 =
            std::mem::transmute(slot(dialog, GET_RESULT));
        let mut path = None;
        if get_result(dialog, &mut item) >= 0 && !item.is_null() {
            let get_name: extern "system" fn(*mut c_void, u32, *mut *mut u16) -> i32 =
                std::mem::transmute(slot(item, GET_DISPLAY_NAME));
            let mut name: *mut u16 = ptr::null_mut();
            if get_name(item, SIGDN_FILESYSPATH, &mut name) >= 0 && !name.is_null() {
                let len = (0..).take_while(|&i| *name.add(i) != 0).count();
                let slice = std::slice::from_raw_parts(name, len);
                path = Some(PathBuf::from(std::ffi::OsString::from_wide(slice)));
                CoTaskMemFree(name as *mut c_void);
            }
            release(item);
        }
        release(dialog);
        path
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::PickKind;
    use std::path::PathBuf;

    pub fn pick(_kind: PickKind, _title: &str, _owner: isize) -> Option<PathBuf> {
        None
    }
}
