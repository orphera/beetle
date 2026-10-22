//! Handing a path to the Windows shell (`ShellExecuteW`): Explorer for a
//! folder (the song menu), the advanced tools (`bpm-gui.exe`, Settings).
//! Going through the shell avoids pulling `std::process` into the app binary.

use std::path::Path;

/// Starts `target` with the shell verb `verb` ("open", "explore"). Returns
/// false when the shell refuses (it reports success as a value above 32).
/// The process is not waited for.
#[cfg(windows)]
pub fn open(verb: &str, target: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    const SW_SHOWNORMAL: i32 = 1;
    let wide = |s: &std::ffi::OsStr| -> Vec<u16> { s.encode_wide().chain(Some(0)).collect() };
    let verb = wide(std::ffi::OsStr::new(verb));
    let file = wide(target.as_os_str());
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    result > 32
}

#[cfg(not(windows))]
pub fn open(_verb: &str, _target: &Path) -> bool {
    false
}

/// The advanced tools (`bpm-gui.exe`), shipped next to the game's executable.
pub fn tools_beside(exe: &Path) -> std::path::PathBuf {
    exe.with_file_name("bpm-gui.exe")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tools_are_the_file_next_to_the_app() {
        assert_eq!(
            tools_beside(Path::new("dir/beetle-app.exe")),
            Path::new("dir/bpm-gui.exe")
        );
    }
}
