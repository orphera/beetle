//! Build script for `bpm-gui`: links the app icon (the same one as the game,
//! from `build_support/app_icon.rs`), read back at run time for the window.

include!("../../build_support/app_icon.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../build_support/app_icon.rs");
    link_app_icon("bpm-gui", false);
}
