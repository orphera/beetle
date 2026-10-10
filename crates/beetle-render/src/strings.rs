//! Every user-visible UI string of beetle-app, in Korean (plan U1d, decision 1).
//!
//! One table, grouped by screen. Terms the BMS / rhythm-game scene keeps in
//! English stay English: judge names, ranks, clear lamps, gauge and modifier
//! names, LN / CN / HCN, BPM, mode labels, difficulty names and key names.
//! Strings with values are templates with `{}` placeholders filled by [`fill`]
//! (or by the small `fn`s below), so every character a user can see lives in
//! this file and the coverage test below can check it against the embedded
//! Korean font (`text::KR_BYTES`). A missing syllable would fall back to GDI.

/// Replaces each `{}` in `template` with the next value, in order.
pub fn fill(template: &str, values: &[&str]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    let mut values = values.iter();
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        out.push_str(values.next().copied().unwrap_or(""));
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

macro_rules! table {
    ($( $(#[$doc:meta])* pub $name:ident = $text:expr; )*) => {
        $( $(#[$doc])* pub const $name: &str = $text; )*

        /// Every entry of the table, for the coverage test.
        #[cfg(test)]
        pub(crate) const ALL: &[(&str, &str)] = &[$((stringify!($name), $name)),*];
    };
}

table! {
    // ---- Shared -------------------------------------------------------
    /// Wordmark on the top bar and the boot screen.
    pub WORDMARK = "BEETLE";
    /// Window title.
    pub WINDOW_TITLE = "Beetle - BMS 리듬 엔진";
    /// Caption of the error dialog.
    pub APP_NAME = "Beetle";
    /// Error dialog when Direct3D 11 cannot start; `{}` is the error.
    pub FATAL_D3D11 = "Beetle이 Direct3D 11(하드웨어 또는 WARP)을 시작하지 못했습니다.\n\n{}";
    pub CANCEL = "취소";
    pub BACK = "뒤로";
    pub LOADING = "불러오는 중";

    // ---- Boot ---------------------------------------------------------
    pub BOOT_TITLE_STARTUP = "시작하는 중";
    pub BOOT_TITLE_RESCAN = "서재를 다시 읽는 중";
    pub BOOT_STATUS_STARTUP = "곡 목록을 읽는 중";
    pub BOOT_STATUS_RESCAN = "곡 폴더를 검색하는 중";

    // ---- Song select --------------------------------------------------
    pub FOLDER = "폴더";
    pub SORT = "정렬";
    pub OPTIONS = "옵션";
    pub SEARCH_PLACEHOLDER = "제목, 아티스트 검색";
    pub PREVIEW = "미리듣기";
    /// Row tag for a chart with no score yet.
    pub NO_PLAY = "기록 없음";
    pub EMPTY_FOLDER = "이 폴더에 곡이 없습니다.";
    pub EMPTY_HINT = "곡 파일(.bms, .bme, .bmsp)을 songs 폴더에 넣고 F5를 눌러 다시 읽으세요.";
    /// `{}` is the search text.
    pub NO_MATCH = "\"{}\"에 맞는 곡이 없습니다.";
    pub SEARCH_HINT = "/ 키로 검색어를 고치고, Esc로 지웁니다.";
    pub PERSONAL_BEST = "최고 기록";
    pub NOT_PLAYED = "아직 플레이하지 않음";
    pub CLEAR_TO_RECORD = "이 채보를 클리어하면 기록이 남습니다.";
    /// `{}` is the gauge name (GROOVE, HARD, ...).
    pub GAUGE_NAME = "{} 게이지";
    /// `{}` is the play count.
    pub PLAY_COUNT = "{}회 플레이";
    pub ACCURACY = "정확도";
    pub MAX_COMBO = "최대 콤보";
    pub MIN_BP = "MIN BP";
    pub PLAY = "플레이";
    pub REPLAY = "리플레이";
    pub AUTO_PLAY = "자동 플레이";
    /// Shown after the folder's song count: `{}` is the count.
    pub SONGS_TOTAL = " / {} 곡";
    pub BPM = "BPM";
    pub NOTES = "노트";
    pub MODE = "모드";

    // Footer key captions (keycaps keep their English key names).
    pub FOOTER_MOVE = "이동";
    pub FOOTER_PLAY = "플레이";
    pub FOOTER_SEARCH = "검색";
    pub FOOTER_FOLDER = "폴더";
    pub FOOTER_SORT = "정렬";
    pub FOOTER_AUTO = "자동";
    pub FOOTER_KEYS = "키 설정";
    pub FOOTER_QUIT = "종료";

    // Folder names (the selector's `<` `>` cycle through these).
    pub FOLDER_ALL = "전체 곡";
    pub FOLDER_5K = "5키";
    pub FOLDER_7K = "7키";
    pub FOLDER_9K = "9키";
    pub FOLDER_10K = "10키";
    pub FOLDER_14K = "14키";
    pub FOLDER_LEVEL = "레벨별";
    pub FOLDER_CLEAR_STATUS = "클리어 상태별";
    /// Folder name for a difficulty table with no name.
    pub FOLDER_TABLE = "난이도표";

    // Sort names.
    pub SORT_TITLE = "제목";
    pub SORT_LEVEL = "레벨";
    pub SORT_CLEAR_LAMP = "클리어";
    pub SORT_SCORE_RATE = "정확도";
    pub SORT_BPM = "BPM";

    // Play options modal.
    pub MODAL_PLAY_OPTIONS = "플레이 옵션";
    pub GROUP_PLAY = "플레이";
    pub GROUP_AUDIO = "소리";
    pub GROUP_LAYOUT = "레이아웃";
    pub GROUP_DISPLAY_SYSTEM = "화면 / 시스템";
    pub GROUP_INPUT_SESSION = "입력 / 세션";
    pub ROW_HI_SPEED = "하이스피드";
    pub ROW_MODIFIER = "모디파이어";
    pub ROW_GAUGE = "게이지";
    pub ROW_LN_MODE = "LN 모드";
    pub ROW_JUDGE_OFFSET = "판정 오프셋";
    pub ROW_MASTER_VOLUME = "전체 볼륨";
    pub ROW_PLAYFIELD = "플레이필드";
    pub ROW_BGA = "BGA";
    pub ROW_TRACK_BGA = "트랙 BGA";
    pub ROW_DISPLAY_MODE = "화면 모드";
    pub ROW_RESOLUTION = "해상도";
    pub ROW_GRAPHICS = "그래픽";
    pub ROW_TARGET_FPS = "목표 FPS";
    pub ROW_KEY_LAYOUT = "키 배치";
    pub ROW_AUTO_PLAY = "자동 플레이";
    pub ROW_START_MEASURE = "시작 마디";
    pub VALUE_ON = "켜짐";
    pub VALUE_OFF = "꺼짐";
    pub VALUE_UNLIMITED = "무제한";
    pub VALUE_CENTER = "가운데";
    /// Resolution label when the window size matches no preset.
    pub RESOLUTION_CUSTOM = "사용자 지정";
    pub DISPLAY_WINDOWED = "창 모드";
    pub DISPLAY_BORDERLESS = "테두리 없는 창";
    pub DISPLAY_FULLSCREEN = "전체 화면";
    pub GPU_AUTO = "자동";
    /// Backend name, kept in English.
    pub GPU_WARP = "WARP (CPU)";
    pub TRACK_BGA_OFF = "꺼짐 (0%)";
    pub TRACK_BGA_LOW = "낮음 (25%)";
    pub TRACK_BGA_MEDIUM = "보통 (50%)";
    pub TRACK_BGA_HIGH = "높음 (75%)";
    /// `{}` is the graphics backend; the note follows it.
    pub VALUE_AFTER_RESTART = "{} (재시작 후 적용)";
    /// `{}` is the FPS number.
    pub VALUE_FPS = "{} FPS";
    /// `{}` is the LN rule AUTO resolves to for the highlighted song.
    pub VALUE_AUTO_RESOLVED = "자동 ({})";
    /// Modal footer hints.
    pub HINT_MOVE = "이동";
    pub HINT_CHANGE = "변경";
    pub HINT_CLOSE = "닫기";

    // Quit confirmation.
    pub QUIT_TITLE = "BEETLE을 종료할까요?";
    pub QUIT_NOTE = "기록과 설정은 이미 저장되어 있습니다.";
    pub QUIT = "종료";

    // ---- Loading ------------------------------------------------------
    pub LOADING_STATUS = "키음을 디코딩하고 오디오를 준비하는 중";
    pub PRACTICE = "연습";

    // ---- Gameplay -----------------------------------------------------
    pub COMBO = "콤보";
    pub NO_BGA = "BGA 없음";
    pub PAUSED = "일시정지";
    pub PAUSE_RESUME = "계속하기";
    pub PAUSE_RESTART = "처음부터";
    pub PAUSE_QUIT = "곡 선택으로";
    pub PAUSE_HINT = "위아래 이동   ENTER 확인   R 처음부터";
    /// Key hint line during a replay or auto play.
    pub HUD_BACK_TO_SELECT = "ESC  곡 선택으로";
    /// Key hint line while playing; `{}` is the key layout.
    pub HUD_KEYS = "키  {}    1/2 하이스피드    F10/F11 커버    ESC 일시정지";
    /// Same, for layouts without cover keys (double play).
    pub HUD_KEYS_NO_COVER = "키  {}    1/2 하이스피드    ESC 일시정지";
    pub KEYS_CUSTOM = "사용자 배치";

    // ---- Result -------------------------------------------------------
    pub RESULT = "결과";
    pub RESULT_SONG_SELECT = "곡 선택";
    pub RESULT_RETRY = "다시 하기";
    pub RESULT_SCREENSHOT = "스크린샷";
    pub SCORE_NOT_SAVED = "기록이 저장되지 않음";
    /// Clear banners. Kept in English next to the clear lamps (FULL COMBO, FAILED).
    pub STAGE_CLEAR = "STAGE CLEAR";
    pub STAGE_FAILED = "STAGE FAILED";
    pub PERFECT = "PERFECT";
    pub FULL_COMBO = "FULL COMBO";
    pub NEW_RECORD = "신기록";
    /// `{}` is the gauge name; `{}` is the rule note (e.g. "LN").
    pub GAUGE_NAME_RULE = "{} 게이지 / {}";
    /// `{}` is the best EX score.
    pub BEST_EX = "최고 {}";
    pub FIRST_PLAY = "첫 플레이";
    pub JUDGE = "판정";
    pub TIMING = "타이밍";
    pub OFFSET = "오프셋";
    pub MISS_COUNT = "MISS 수";

    // ---- Key config ---------------------------------------------------
    pub KEY_CONFIG = "키 설정";
    pub KEY_LANE = "레인";
    pub SET_KEY = "키 지정";
    pub ADD_KEY = "키 추가";
    pub CLEAR_LANE = "비우기";
    pub PRESET = "프리셋";
    pub SCRATCH = "스크래치";
    pub FORM_8K = "8K 형태";
    pub RESET = "초기화";
    pub ANY_KEY = "아무 키";
    pub BIND = "지정";
    pub SELECTED_LANE = "선택한 레인";
    pub KEYS_CAPTION = "키";
    pub SIDE_LEFT = "왼쪽";
    pub SIDE_RIGHT = "오른쪽";
    pub LAYOUT_TRIGGERS = "왼쪽, 오른쪽 트리거 사이에 키 6개 (F3: 일자 배치)";
    /// `{}` is the scratch side.
    pub LAYOUT_8K = "키 8개를 한 줄로, 스크래치는 {} (F2: 교체, F3: 키 6개 + 트리거)";
    pub LAYOUT_STRAIGHT = "키를 한 줄로 배치";
    /// `{}` is the scratch side.
    pub LAYOUT_SCRATCH = "스크래치는 {} (F2: 교체)";
    pub LAYOUT_PER_MODE = "키 모드마다 배치를 따로 저장";
    pub REBIND_REPLACE = "이 레인에 쓸 키를 누르세요. 레인의 기존 키를 대신하고, 다른 곳에서 쓰던 키는 여기로 옮겨집니다.";
    pub REBIND_ADD = "이 레인에 다른 키를 하나 더 누르세요. 다른 레인이 쓰던 키는 여기로 옮겨집니다.";
    pub REBIND_SELECTED = "Enter로 키를 지정하고, A로 하나 더 추가하며, Backspace로 레인을 비웁니다.";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::KR_BYTES;
    use fontdue::{Font, FontSettings};

    /// Every character of every string must be in the embedded KR font, or
    /// it would be drawn by the GDI fallback and look different. Symbols the
    /// KR font lacks must be replaced by plain words or ASCII.
    #[test]
    fn every_character_is_in_the_embedded_kr_font() {
        let font = Font::from_bytes(KR_BYTES, FontSettings::default()).expect("KR font");
        let mut missing = Vec::new();
        for (name, text) in ALL {
            for c in text.chars() {
                if c.is_control() || c == ' ' {
                    continue;
                }
                if font.lookup_glyph_index(c) == 0 {
                    missing.push(format!("{name}: {c:?} (U+{:04X})", c as u32));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "glyphs missing from the KR font:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn fill_substitutes_placeholders_in_order() {
        assert_eq!(
            fill("{} 게이지  ·  {}", &["GROOVE", "LN"]),
            "GROOVE 게이지  ·  LN"
        );
        assert_eq!(fill("\"{}\"에 맞는 곡", &["zzz"]), "\"zzz\"에 맞는 곡");
        assert_eq!(fill("no slot", &["x"]), "no slot");
    }
}
