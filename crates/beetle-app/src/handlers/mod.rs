pub mod gameplay;
pub mod key_config;
pub mod options;
pub mod result;
pub mod song_select;

pub use gameplay::handle_gameplay_input;
pub use key_config::handle_key_config_input;
pub use result::handle_result_input;
pub use song_select::handle_song_select_input;
