//! Opening `.bmsp` files with the game from Explorer (plan S5). Registered
//! for the current user only (HKCU\Software\Classes), and only when the
//! Settings row is turned on; turning it off removes what was written.
//! No crate: advapi32 and shell32 are called directly.

/// The ProgID the `.bmsp` extension points at.
const PROG_ID: &str = "Beetle.Package";

/// The `.bmsp` file icon: icon group 2 in the game's exe (`build_support/app_icon.rs`).
pub fn package_icon(exe: &str) -> String {
    format!("\"{exe}\",-2")
}

/// Points `.bmsp` at the current file icon when the association is on, so an
/// association made before the file icon existed (or by a moved exe) shows it.
pub fn refresh_icon() {
    if !is_registered() {
        return;
    }
    if let Some(exe) = current_exe() {
        let key = format!(r"Software\Classes\{PROG_ID}\DefaultIcon");
        if imp::get(&key).as_deref() != Some(package_icon(&exe).as_str()) {
            imp::set(&key, &package_icon(&exe));
            imp::notify();
        }
    }
}

/// The open command for `exe`: the file goes in as the first argument.
pub fn open_command(exe: &str) -> String {
    format!("\"{exe}\" \"%1\"")
}

/// Whether `.bmsp` opens with this executable.
pub fn is_registered() -> bool {
    let Some(exe) = current_exe() else {
        return false;
    };
    imp::get(&format!(r"Software\Classes\{PROG_ID}\shell\open\command"))
        .is_some_and(|cmd| cmd.eq_ignore_ascii_case(&open_command(&exe)))
        && imp::get(r"Software\Classes\.bmsp").as_deref() == Some(PROG_ID)
}

/// Registers or removes the association. Returns whether it worked.
pub fn set(on: bool) -> bool {
    let ok = if on {
        let Some(exe) = current_exe() else {
            return false;
        };
        imp::set(r"Software\Classes\.bmsp", PROG_ID)
            && imp::set(
                &format!(r"Software\Classes\{PROG_ID}"),
                "Beetle song package",
            )
            && imp::set(
                &format!(r"Software\Classes\{PROG_ID}\DefaultIcon"),
                &package_icon(&exe),
            )
            && imp::set(
                &format!(r"Software\Classes\{PROG_ID}\shell\open\command"),
                &open_command(&exe),
            )
    } else {
        // `.bmsp` is left alone when another program has taken it since.
        let ours = imp::get(r"Software\Classes\.bmsp").as_deref() == Some(PROG_ID);
        imp::delete_tree(&format!(r"Software\Classes\{PROG_ID}"))
            && (!ours || imp::delete_tree(r"Software\Classes\.bmsp"))
    };
    imp::notify();
    ok
}

fn current_exe() -> Option<String> {
    std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::ptr;

    const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
    const REG_SZ: u32 = 1;
    const RRF_RT_REG_SZ: u32 = 0x2;
    const ERROR_SUCCESS: i32 = 0;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegSetKeyValueW(
            key: isize,
            sub_key: *const u16,
            value_name: *const u16,
            kind: u32,
            data: *const c_void,
            bytes: u32,
        ) -> i32;
        fn RegGetValueW(
            key: isize,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut c_void,
            bytes: *mut u32,
        ) -> i32;
        fn RegDeleteTreeW(key: isize, sub_key: *const u16) -> i32;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SHChangeNotify(event: i32, flags: u32, a: *const c_void, b: *const c_void);
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    /// The default value of `HKCU\<key>`.
    pub fn get(key: &str) -> Option<String> {
        let key = wide(key);
        let mut buf = [0u16; 1024];
        let mut bytes = (buf.len() * 2) as u32;
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                ptr::null(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                buf.as_mut_ptr() as *mut c_void,
                &mut bytes,
            )
        };
        if r != ERROR_SUCCESS {
            return None;
        }
        let len = (bytes as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..len.min(buf.len())]))
    }

    /// Sets the default value of `HKCU\<key>`, creating the key.
    pub fn set(key: &str, value: &str) -> bool {
        let key = wide(key);
        let value = wide(value);
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                ptr::null(),
                REG_SZ,
                value.as_ptr() as *const c_void,
                (value.len() * 2) as u32,
            ) == ERROR_SUCCESS
        }
    }

    /// Removes `HKCU\<key>` and everything under it. A missing key is fine.
    pub fn delete_tree(key: &str) -> bool {
        let key = wide(key);
        let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) };
        r == ERROR_SUCCESS || r == ERROR_FILE_NOT_FOUND
    }

    /// Tells Explorer the associations changed, so icons update.
    pub fn notify() {
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, 0, ptr::null(), ptr::null()) };
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn get(_key: &str) -> Option<String> {
        None
    }
    pub fn set(_key: &str, _value: &str) -> bool {
        false
    }
    pub fn delete_tree(_key: &str) -> bool {
        false
    }
    pub fn notify() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_icon_is_the_second_icon_group_of_the_exe() {
        assert_eq!(
            package_icon(r"C:\Games\Beetleeetle-app.exe"),
            r#""C:\Games\Beetleeetle-app.exe",-2"#
        );
    }

    #[test]
    fn the_command_quotes_both_paths() {
        assert_eq!(
            open_command(r"C:\Games\Beetle\beetle-app.exe"),
            r#""C:\Games\Beetle\beetle-app.exe" "%1""#
        );
    }
}
