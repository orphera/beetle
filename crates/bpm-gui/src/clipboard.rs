/// Pure lightweight Win32 clipboard reader without external crate dependencies.
#[cfg(target_os = "windows")]
pub fn get_clipboard_text() -> Option<String> {
    use std::ffi::c_void;
    use std::ptr;

    type HWND = *mut c_void;
    type HANDLE = *mut c_void;
    type UINT = u32;
    type BOOL = i32;

    const CF_UNICODETEXT: UINT = 13;

    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(hWndNewOwner: HWND) -> BOOL;
        fn CloseClipboard() -> BOOL;
        fn GetClipboardData(uFormat: UINT) -> HANDLE;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalLock(hMem: HANDLE) -> *mut u16;
        fn GlobalUnlock(hMem: HANDLE) -> BOOL;
    }

    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }

        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            CloseClipboard();
            return None;
        }

        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            CloseClipboard();
            return None;
        }

        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }

        let slice = std::slice::from_raw_parts(ptr, len);
        let text = String::from_utf16_lossy(slice);

        GlobalUnlock(handle);
        CloseClipboard();

        let trimmed = text.trim();
        let stripped = if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        };

        if stripped.is_empty() {
            None
        } else {
            Some(stripped.to_string())
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_clipboard_text() -> Option<String> {
    None
}
