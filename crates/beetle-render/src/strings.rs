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

    // ---- Toasts (short notices over the menus) ------------------------
    /// `{}` is the number of songs read.
    pub TOAST_RESCAN_DONE = "곡 {}개를 읽었습니다";
    /// `{}` is the file name of the saved screenshot.
    pub TOAST_SCREENSHOT_SAVED = "스크린샷을 저장했습니다: {}";
    /// `{}` is the error.
    pub TOAST_SCREENSHOT_FAILED = "스크린샷을 저장하지 못했습니다: {}";
    /// Shown when leaving Settings with a value that only applies after a restart.
    pub TOAST_RESTART_NEEDED = "재시작 후 적용됩니다";
    pub TOAST_UNSUPPORTED_FILE = "지원하지 않는 파일입니다 (.bmsp, .bms, .bme, .bml, .pms)";
    /// `{}` is the file name.
    pub TOAST_OPEN_FAILED = "곡을 열지 못했습니다: {}";
    /// `{}` is the layout name (key mode and preset, e.g. "7K HomeRow (...)").
    pub TOAST_PRESET = "키 배치: {} 프리셋";
    /// `{}` is the layout name (key mode and preset).
    pub TOAST_LAYOUT_RESET = "키 배치를 {} 프리셋으로 되돌렸습니다";

    // ---- Boot ---------------------------------------------------------
    pub BOOT_TITLE_STARTUP = "시작하는 중";
    pub BOOT_TITLE_RESCAN = "서재를 다시 읽는 중";
    pub BOOT_STATUS_STARTUP = "곡 목록을 읽는 중";
    pub BOOT_STATUS_RESCAN = "곡 폴더를 검색하는 중";

    // ---- Song select --------------------------------------------------
    pub FOLDER = "폴더";
    pub SORT = "정렬";
    pub OPTIONS = "옵션";
    /// Top bar button for the Settings screen (F4).
    pub SETTINGS = "설정";
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
    pub FOOTER_HELP = "도움말";

    // Folder tree (song select). Mode tags (7K) and lamp names (PERFECT) stay in English.
    /// The root of the breadcrumb, above the top-level folders.
    pub FOLDER_ROOT = "전체";
    pub FOLDER_ALL = "전체 곡";
    pub FOLDER_MODE = "키 모드";
    pub FOLDER_LEVEL = "레벨";
    pub FOLDER_LAMP = "클리어 램프";
    pub FOLDER_TABLE = "난이도표";
    /// Detail panel and footer count of a folder: `{}` is the song count.
    pub FOLDER_SONGS = "{} 곡";
    /// Footer count while the list is folders only: `{}` is the folder count.
    pub FOLDER_COUNT = "{} 폴더";
    pub FOLDER_OPEN_HINT = "ENTER로 열고, BKSP로 위 폴더로 나갑니다.";
    pub FOOTER_OPEN = "열기";

    // Sort names.
    pub SORT_TITLE = "제목";
    pub SORT_LEVEL = "레벨";
    pub SORT_CLEAR_LAMP = "클리어";
    pub SORT_SCORE_RATE = "정확도";
    pub SORT_BPM = "BPM";

    // Filter row and result count (song select, F10 focuses the row).
    pub FILTER = "필터";
    pub FILTER_LEVEL = "레벨";
    /// The level bounds while they are not set.
    pub FILTER_LEVEL_MIN_ANY = "최소";
    pub FILTER_LEVEL_MAX_ANY = "최대";
    pub FILTER_UNPLAYED = "미플레이만";
    pub FILTER_UNCLEARED = "미클리어만";
    pub FILTER_RESET = "초기화";
    /// Empty list while a filter is on and there is no search.
    pub FILTER_NO_MATCH = "필터에 맞는 곡이 없습니다.";
    pub FILTER_EMPTY_HINT = "필터를 바꾸거나 초기화하세요.";
    /// `{}` is the number of songs found.
    pub RESULT_COUNT = "{}곡 찾음";

    // Help overlay (song select, ? key). Keys stay English keycaps.
    pub HELP_TITLE = "도움말";
    pub HELP_CLOSE = "? 또는 ESC로 닫습니다";
    pub HELP_GROUP_MOVE = "이동";
    pub HELP_GROUP_FOLDER = "폴더";
    pub HELP_GROUP_SONG = "곡";
    pub HELP_GROUP_OPTIONS = "옵션과 설정";
    pub HELP_GROUP_FILTER = "필터와 정렬";
    pub HELP_GROUP_MISC = "기타";
    pub HELP_CAP_MOUSE = "마우스";
    pub HELP_CAP_DROP = "끌어다 놓기";
    pub HELP_MOVE_ONE = "한 줄씩 움직입니다";
    pub HELP_MOVE_PAGE = "10줄씩 움직입니다";
    pub HELP_MOVE_ENDS = "처음과 끝으로 갑니다";
    pub HELP_MOUSE_WHEEL = "휠로 목록을 굴립니다";
    pub HELP_MOUSE_ROW = "줄을 누르면 고릅니다";
    pub HELP_FOLDER_UP = "한 단계 위 폴더로 갑니다";
    pub HELP_FOLDER_OPEN = "폴더는 열고 곡은 재생합니다";
    pub HELP_FOLDER_SIDE = "같은 단계의 이전과 다음 폴더";
    pub HELP_MOUSE_SIDE = "상단 < > 버튼으로 옆 폴더";
    pub HELP_MOUSE_CRUMB = "빵부스러기를 누르면 그 폴더";
    pub HELP_SONG_PLAY = "선택한 곡을 플레이합니다";
    pub HELP_SONG_AUTO = "자동 플레이를 켜고 끕니다";
    pub HELP_SONG_REPLAY = "저장된 리플레이를 재생합니다";
    pub HELP_SONG_CHART = "묶음 행의 채보를 바꿉니다";
    pub HELP_MOUSE_AGAIN = "고른 줄을 다시 누르면 재생";
    pub HELP_OPTIONS = "플레이 옵션을 엽니다";
    pub HELP_SETTINGS = "설정 화면을 엽니다";
    pub HELP_KEYS = "키 설정을 엽니다";
    pub HELP_MOUSE_BUTTONS = "상단의 옵션, 설정 버튼";
    pub HELP_SEARCH = "검색창을 엽니다";
    pub HELP_SORT = "정렬 기준을 바꿉니다";
    pub HELP_FILTER = "필터 줄에 초점을 줍니다";
    pub HELP_MOUSE_FILTER = "정렬, 필터 칩, 검색창 누르기";
    pub HELP_BACK_QUIT = "한 단계 위, 루트에서는 종료";
    pub HELP_RESCAN = "곡 목록을 다시 읽습니다";
    pub HELP_HELP = "도움말을 열고 닫습니다";
    pub HELP_DROP = "파일을 끌어다 놓으면 곡을 엽니다";

    // First run: the library has no songs (only the demo). The guide lists three ways to add songs.
    pub GUIDE_TITLE = "아직 곡이 없습니다";
    pub GUIDE_LEAD = "곡을 넣는 방법은 세 가지입니다.";
    pub GUIDE_MANAGER = "곡 관리자로 곡 패키지(.bmsp)를 설치합니다.";
    pub GUIDE_DROP = "곡 파일(.bmsp, .bms)이나 폴더를 창에 끌어다 놓습니다.";
    pub GUIDE_FOLDER = "곡 파일을 songs 폴더에 넣습니다.";
    pub GUIDE_RESCAN = "곡을 넣은 뒤 다시 읽습니다.";
    pub GUIDE_BTN_MANAGER = "곡 관리자 열기";
    pub GUIDE_BTN_FOLDER = "songs 폴더 열기";
    pub GUIDE_BTN_RESCAN = "다시 읽기";

    // Drag overlay: a file is dragged over the window (song select).
    pub DROP_TITLE = "여기에 놓으면 곡을 엽니다";
    pub DROP_NOTE = ".bmsp 패키지나 .bms 채보를 놓으면 바로 엽니다.";

    // Opening the song manager or the songs folder failed.
    pub TOAST_MANAGER_MISSING = "곡 관리자(bpm-gui.exe)를 찾지 못했습니다";
    pub TOAST_FOLDER_FAILED = "songs 폴더를 열지 못했습니다";

    // Play options panel (song select, TAB / O): per-play values only.
    pub MODAL_PLAY_OPTIONS = "플레이 옵션";
    pub GROUP_PLAY = "플레이";
    pub GROUP_SESSION = "세션";
    pub GROUP_DISPLAY = "화면";
    pub GROUP_AUDIO = "소리";
    pub GROUP_JUDGE = "판정";
    pub GROUP_LAYOUT = "레이아웃";
    pub GROUP_INPUT = "입력";
    pub ROW_GREEN = "그린 넘버";
    pub ROW_LANE_COVER = "레인 커버";
    pub ROW_MODIFIER = "모디파이어";
    pub ROW_GAUGE = "게이지";
    pub ROW_LN_MODE = "LN 모드";
    pub ROW_AUTO_PLAY = "자동 플레이";
    pub ROW_START_MEASURE = "시작 마디";
    pub ROW_JUDGE_OFFSET = "판정 오프셋";
    pub ROW_MASTER_VOLUME = "전체 볼륨";
    pub ROW_PLAYFIELD = "플레이필드";
    pub ROW_SCRATCH = "스크래치";
    pub ROW_BGA = "BGA";
    pub ROW_TRACK_BGA = "트랙 BGA";
    pub ROW_KEY_HINT = "플레이 중 키 안내";
    pub KEY_HINT_FIRST = "처음 3초만";
    pub KEY_HINT_ALWAYS = "항상";
    pub KEY_HINT_OFF = "끄기";
    pub ROW_DISPLAY_MODE = "화면 모드";
    pub ROW_RESOLUTION = "해상도";
    pub ROW_GRAPHICS = "그래픽";
    pub ROW_TARGET_FPS = "목표 FPS";
    pub ROW_KEY_LAYOUT = "키 배치";
    pub VALUE_ON = "켜짐";
    pub VALUE_OFF = "꺼짐";
    pub VALUE_UNLIMITED = "무제한";
    pub VALUE_CENTER = "가운데";
    /// A row that does not apply to the selected song's key mode.
    pub VALUE_NOT_APPLICABLE = "해당 없음";
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
    /// `{}` is the green number in milliseconds.
    pub VALUE_MS = "{} ms";
    /// Option chip above PLAY; `{}` is the green number in milliseconds.
    pub CHIP_GREEN = "그린 {}";
    /// Modal footer hints.
    pub HINT_MOVE = "이동";
    pub HINT_CHANGE = "변경";
    pub HINT_CLOSE = "닫기";
    pub HINT_SELECT = "선택";

    // Help sentences: one line under the highlighted row (plain Korean).
    pub HELP_GREEN = "노트가 화면에 나타나서 판정선에 닿기까지의 시간입니다. 작을수록 빠릅니다. 레인 커버를 바꿔도 유지됩니다.";
    pub HELP_LANE_COVER = "레인 위쪽을 가려 노트가 보이는 구간을 줄입니다. 0%는 가리지 않습니다. 플레이 중 F10, F11로도 바꿀 수 있습니다.";
    pub HELP_MODIFIER = "노트 배치를 바꿉니다. 미러는 좌우를 뒤집고, 랜덤 계열은 레인 순서를 섞습니다.";
    pub HELP_GAUGE = "점수 게이지의 방식입니다. EASY와 GROOVE는 관대하고, HARD와 HAZARD는 실수에 엄격합니다.";
    pub HELP_LN_MODE = "롱노트 판정 방식입니다. AUTO는 곡 데이터에 맞추고, LN과 CN은 어떤 곡이든 그 방식으로 고정합니다.";
    pub HELP_AUTO_PLAY = "켜면 키를 누르지 않아도 곡이 끝까지 연주됩니다. 자동 플레이 결과는 기록으로 저장되지 않습니다.";
    pub HELP_START_MEASURE = "이 마디부터 곡을 시작합니다. 0이면 처음부터입니다. 0이 아니면 연습으로 기록되지 않습니다.";

    // Settings screen (its title is SETTINGS above).
    pub HELP_DISPLAY_MODE = "창 모드, 테두리 없는 창, 전체 화면 중에서 고릅니다. 바로 적용됩니다.";
    pub HELP_RESOLUTION = "창 크기나 전체 화면 해상도를 고릅니다. 창 모드는 16:9 크기만 고를 수 있습니다.";
    pub HELP_GRAPHICS = "그림을 그리는 장치입니다. 자동은 그래픽 카드를 쓰고, WARP는 CPU로 그립니다. 바꾸면 재시작 후 적용됩니다.";
    pub HELP_TARGET_FPS = "플레이 화면의 최대 프레임입니다. 무제한은 제한하지 않고, 60은 화면 주사율에 맞춥니다.";
    pub HELP_MASTER_VOLUME = "키음과 배경음을 포함한 전체 소리 크기입니다. 최대 200%까지 올릴 수 있습니다.";
    pub HELP_JUDGE_OFFSET = "판정 기준 시각을 앞뒤로 옮깁니다. 범위는 -100에서 +100 ms입니다. ENTER로 박자에 맞춰 측정할 수 있습니다.";
    pub HELP_PLAYFIELD = "플레이필드를 화면 가운데, 왼쪽, 오른쪽 중 어디에 둘지 정합니다.";
    pub HELP_SCRATCH = "스크래치 레인을 왼쪽이나 오른쪽 가장자리에 둡니다. 5K, 7K, 8K에서만 적용됩니다.";
    pub HELP_BGA = "곡의 배경 영상과 이미지를 보여 줍니다. 끄면 이 파일들을 읽지 않아 메모리와 CPU를 아낍니다.";
    pub HELP_KEY_HINT = "플레이 화면 아래의 키 안내 줄을 정합니다. 처음 3초만이면 시작 뒤 3초 동안만 보이고 사라집니다.";
    pub HELP_TRACK_BGA = "배경 영상을 노트 레인 뒤에 얼마나 진하게 비출지 정합니다. 꺼짐이면 보이지 않습니다.";
    pub HELP_KEY_LAYOUT = "선택한 곡의 키 모드에서 쓸 키 배치를 고릅니다. ENTER로 키 설정 화면을 엽니다.";

    // Judge offset calibration (a Settings sub-screen).
    pub CALIBRATE_TITLE = "판정 오프셋 측정";
    pub CALIBRATE_INSTRUCTION = "소리에 맞춰 스페이스를 누르세요";
    pub CALIBRATE_DONE_TITLE = "측정이 끝났습니다";
    pub CALIBRATE_NO_AUDIO = "오디오 출력 장치를 열지 못해 측정할 수 없습니다";
    pub CALIBRATE_COUNT_IN = "준비";
    pub CALIBRATE_MEASURING = "측정 중";
    pub CALIBRATE_PROGRESS = "진행";
    pub CALIBRATE_MEAN = "평균";
    pub CALIBRATE_SPREAD = "편차";
    pub CALIBRATE_SUGGEST = "제안값";
    pub CALIBRATE_EARLY = "빠름";
    pub CALIBRATE_LATE = "늦음";
    pub CALIBRATE_EXCLUDED = "제외된 입력은 흐리게 표시합니다";
    pub CALIBRATE_HELP = "준비 박 네 개 뒤부터 입력을 16번 셉니다. 한 박에 한 번만 세고, 150 ms보다 멀리 떨어진 입력은 무시합니다.";
    pub CALIBRATE_HELP_DONE = "제안값은 평균 차이의 반대 값입니다. 적용하면 판정 오프셋이 이 값이 됩니다.";
    pub CALIBRATE_HELP_NO_AUDIO = "설정은 바뀌지 않았습니다. ESC로 설정 화면에 돌아가세요.";
    pub CALIBRATE_APPLY = "적용";
    pub CALIBRATE_RETRY = "다시";
    pub CALIBRATE_CANCEL = "취소";
    pub CALIBRATE_HINT_TAP = "누르기";
    pub TOAST_CALIBRATE_APPLIED = "판정 오프셋을 {} ms로 맞췄습니다";

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
    pub HUD_KEYS = "키  {}    1/2 그린 넘버    F10/F11 커버    ESC 일시정지";
    /// Same, for layouts without cover keys (double play).
    pub HUD_KEYS_NO_COVER = "키  {}    1/2 그린 넘버    ESC 일시정지";
    pub KEYS_CUSTOM = "사용자 배치";
    /// Shown from the song start until a second before the first note.
    pub READY = "READY";
    pub READY_HINT = "F3/F4 그린 넘버 / F10/F11 레인 커버";
    /// Readout over the lane after a green number change (`{}` = ms).
    pub READOUT_GREEN = "그린 {} ms";
    /// Readout over the lane after a lane cover change (`{}` = percent).
    pub READOUT_COVER = "커버 {}%";
    pub TIMELINE_TITLE = "판정 타임라인";
    pub TIMELINE_RANGE = "최근 8초";
    pub TIMING_FAST = "FAST";
    pub TIMING_SLOW = "SLOW";

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
