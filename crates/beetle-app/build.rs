//! Build script for the `beetle-app` executable.
//!
//! On MSVC, the exe's static imports (hid.dll, user32.dll, ...) are resolved
//! from the system directory only (`DEPENDENTLOADFLAG` with
//! `LOAD_LIBRARY_SEARCH_SYSTEM32`, 0x800), so a DLL planted beside the exe in
//! a user-writable folder is never loaded. XInput is loaded the same way
//! (`xinput.rs`). Other toolchains do not take the MSVC linker flag.
//!
//! The app icon comes from `build_support/app_icon.rs` (shared with `bpm-gui`):
//! icon group 1, read back at run time for the window (`APP_ICON_ID` in `main.rs`).

include!("../../build_support/app_icon.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../build_support/app_icon.rs");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=beetle-app=/DEPENDENTLOADFLAG:0x800");
    }
    link_app_icon("beetle-app");
}
