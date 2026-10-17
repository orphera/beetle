//! Every user-visible UI string of beetle-app, in Korean (plan U1d, decision 1).
//!
//! One table, grouped by screen. Terms the BMS / rhythm-game scene keeps in
//! English stay English: judge names, ranks, clear lamps, gauge and modifier
//! names, LN / CN / HCN, BPM, mode labels, difficulty names and key names.
//! Strings with values are templates with `{}` placeholders filled by [`fill`]
//! (or by the small `fn`s below), so every character a user can see lives in
//! this file and the coverage test below can check it against the embedded
//! Korean font (`text::KR_BYTES`). A missing syllable would fall back to GDI.

// Accessors are named like the Korean-era constants (`WORDMARK()`), so the
// call sites read the same as before.
#![allow(non_snake_case)]

use std::sync::atomic::{AtomicU8, Ordering};

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

/// The UI languages. Korean is the source language and the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Lang {
    Ko = 0,
    En = 1,
    Ja = 2,
}

/// The language every string accessor reads. Set once at startup and when
/// the Settings language row changes.
static LANG: AtomicU8 = AtomicU8::new(Lang::Ko as u8);

/// Sets the language of every string accessor from now on.
pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

/// The language the string accessors currently return.
pub fn lang() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::En,
        2 => Lang::Ja,
        _ => Lang::Ko,
    }
}

#[inline]
fn pick(texts: [&'static str; 3]) -> &'static str {
    texts[lang() as usize]
}

macro_rules! table {
    ($( $(#[$doc:meta])* pub $name:ident = ko $ko:literal, en $en:literal, ja $ja:literal; )*) => {
        $(
            $(#[$doc])*
            pub fn $name() -> &'static str {
                pick([$ko, $en, $ja])
            }
        )*

        /// Every entry in every language, for the coverage tests.
        #[cfg(test)]
        pub(crate) const ALL: &[(&str, [&str; 3])] = &[$((stringify!($name), [$ko, $en, $ja])),*];
    };
}

table! {
    // ---- Shared -------------------------------------------------------
    /// Wordmark on the top bar and the boot screen.
    pub WORDMARK =
        ko "BEETLE",
        en "BEETLE",
        ja "BEETLE";
    /// Window title.
    pub WINDOW_TITLE =
        ko "Beetle - BMS 리듬 엔진",
        en "Beetle - BMS 리듬 엔진",
        ja "Beetle - BMS 리듬 엔진";
    /// Caption of the error dialog.
    pub APP_NAME =
        ko "Beetle",
        en "Beetle",
        ja "Beetle";
    /// Error dialog when Direct3D 11 cannot start; `{}` is the error.
    pub FATAL_D3D11 =
        ko "Beetle이 Direct3D 11(하드웨어 또는 WARP)을 시작하지 못했어요.\n\n{}",
        en "Beetle이 Direct3D 11(하드웨어 또는 WARP)을 시작하지 못했어요.\n\n{}",
        ja "Beetle이 Direct3D 11(하드웨어 또는 WARP)을 시작하지 못했어요.\n\n{}";
    pub CANCEL =
        ko "취소",
        en "취소",
        ja "취소";
    pub BACK =
        ko "뒤로",
        en "뒤로",
        ja "뒤로";
    pub LOADING =
        ko "불러오는 중",
        en "불러오는 중",
        ja "불러오는 중";

    // ---- Toasts (short notices over the menus) ------------------------
    /// `{}` is the number of songs read.
    pub TOAST_RESCAN_DONE =
        ko "곡 {}개를 읽었어요",
        en "곡 {}개를 읽었어요",
        ja "곡 {}개를 읽었어요";
    /// `{}` is the file name of the saved screenshot.
    pub TOAST_SCREENSHOT_SAVED =
        ko "스크린샷을 저장했어요: {}",
        en "스크린샷을 저장했어요: {}",
        ja "스크린샷을 저장했어요: {}";
    /// `{}` is the error.
    pub TOAST_SCREENSHOT_FAILED =
        ko "스크린샷을 저장하지 못했어요: {}",
        en "스크린샷을 저장하지 못했어요: {}",
        ja "스크린샷을 저장하지 못했어요: {}";
    /// Shown when leaving Settings with a value that only applies after a restart.
    pub TOAST_RESTART_NEEDED =
        ko "재시작 후 적용돼요",
        en "재시작 후 적용돼요",
        ja "재시작 후 적용돼요";
    pub TOAST_UNSUPPORTED_FILE =
        ko "지원하지 않는 파일이에요 (.bmsp, .bms, .bme, .bml, .pms)",
        en "지원하지 않는 파일이에요 (.bmsp, .bms, .bme, .bml, .pms)",
        ja "지원하지 않는 파일이에요 (.bmsp, .bms, .bme, .bml, .pms)";
    /// `{}` is the file name of the package.
    pub TOAST_UNINSTALLED_ONE =
        ko "설치되지 않은 패키지: {} / bpm install로 설치하세요",
        en "설치되지 않은 패키지: {} / bpm install로 설치하세요",
        ja "설치되지 않은 패키지: {} / bpm install로 설치하세요";
    /// The first `{}` is the count, the second the first file name.
    pub TOAST_UNINSTALLED_MANY =
        ko "설치되지 않은 패키지 {}개 ({} 외) / bpm install로 설치하세요",
        en "설치되지 않은 패키지 {}개 ({} 외) / bpm install로 설치하세요",
        ja "설치되지 않은 패키지 {}개 ({} 외) / bpm install로 설치하세요";
    /// `{}` is the file name.
    pub TOAST_OPEN_FAILED =
        ko "곡을 열지 못했어요: {}",
        en "곡을 열지 못했어요: {}",
        ja "곡을 열지 못했어요: {}";
    /// `{}` is the chart file name. Shown when a chart cannot be read or parsed; nothing plays.
    pub TOAST_CHART_LOAD_FAILED =
        ko "채보를 불러오지 못했어요: {}",
        en "채보를 불러오지 못했어요: {}",
        ja "채보를 불러오지 못했어요: {}";
    /// `{}` is the layout name (key mode and preset, e.g. "7K HomeRow (...)").
    pub TOAST_PRESET =
        ko "키 배치: {} 프리셋",
        en "키 배치: {} 프리셋",
        ja "키 배치: {} 프리셋";
    /// `{}` is the layout name (key mode and preset).
    pub TOAST_LAYOUT_RESET =
        ko "키 배치를 {} 프리셋으로 되돌렸어요",
        en "키 배치를 {} 프리셋으로 되돌렸어요",
        ja "키 배치를 {} 프리셋으로 되돌렸어요";

    // ---- Title (the first screen after the library is read) -----------
    /// Under the wordmark; `{}` is the app version.
    pub TITLE_TAGLINE =
        ko "BMS Player  v{}",
        en "BMS Player  v{}",
        ja "BMS Player  v{}";
    /// `{}` is the number of songs in the library.
    pub TITLE_SONGS =
        ko "곡 {}개",
        en "곡 {}개",
        ja "곡 {}개";
    /// The first menu entry: song select. Also the label after the prompt's keycap.
    pub TITLE_START =
        ko "시작",
        en "시작",
        ja "시작";
    /// The prompt before its keycap: PRESS [ENTER] 시작.
    pub TITLE_PRESS =
        ko "PRESS",
        en "PRESS",
        ja "PRESS";
    /// The keycap of the prompt (the key's own name).
    pub TITLE_ENTER =
        ko "ENTER",
        en "ENTER",
        ja "ENTER";

    // ---- Boot ---------------------------------------------------------
    pub BOOT_TITLE_STARTUP =
        ko "시작하는 중",
        en "시작하는 중",
        ja "시작하는 중";
    pub BOOT_TITLE_RESCAN =
        ko "서재를 다시 읽는 중",
        en "서재를 다시 읽는 중",
        ja "서재를 다시 읽는 중";
    pub BOOT_STATUS_STARTUP =
        ko "곡 목록을 읽는 중",
        en "곡 목록을 읽는 중",
        ja "곡 목록을 읽는 중";
    pub BOOT_STATUS_RESCAN =
        ko "곡 폴더를 검색하는 중",
        en "곡 폴더를 검색하는 중",
        ja "곡 폴더를 검색하는 중";

    // ---- Song select --------------------------------------------------
    pub FOLDER =
        ko "폴더",
        en "폴더",
        ja "폴더";
    pub SORT =
        ko "정렬",
        en "정렬",
        ja "정렬";
    pub OPTIONS =
        ko "옵션",
        en "옵션",
        ja "옵션";
    /// Top bar button for the Settings screen (F4).
    pub SETTINGS =
        ko "설정",
        en "설정",
        ja "설정";
    pub SEARCH_PLACEHOLDER =
        ko "제목, 아티스트 검색",
        en "제목, 아티스트 검색",
        ja "제목, 아티스트 검색";
    pub PREVIEW =
        ko "미리듣기",
        en "미리듣기",
        ja "미리듣기";
    /// Row tag for a chart with no score yet.
    pub NO_PLAY =
        ko "기록 없음",
        en "기록 없음",
        ja "기록 없음";
    pub EMPTY_FOLDER =
        ko "이 폴더에 곡이 없어요.",
        en "이 폴더에 곡이 없어요.",
        ja "이 폴더에 곡이 없어요.";
    pub EMPTY_HINT =
        ko "곡 파일(.bms, .bme, .bmsp)을 songs 폴더에 넣고 F5를 눌러 다시 읽으세요.",
        en "곡 파일(.bms, .bme, .bmsp)을 songs 폴더에 넣고 F5를 눌러 다시 읽으세요.",
        ja "곡 파일(.bms, .bme, .bmsp)을 songs 폴더에 넣고 F5를 눌러 다시 읽으세요.";
    /// `{}` is the search text.
    pub NO_MATCH =
        ko "\"{}\"에 맞는 곡이 없어요.",
        en "\"{}\"에 맞는 곡이 없어요.",
        ja "\"{}\"에 맞는 곡이 없어요.";
    pub SEARCH_HINT =
        ko "/ 키로 검색어를 고치고, Esc로 지워요.",
        en "/ 키로 검색어를 고치고, Esc로 지워요.",
        ja "/ 키로 검색어를 고치고, Esc로 지워요.";
    pub PERSONAL_BEST =
        ko "최고 기록",
        en "최고 기록",
        ja "최고 기록";
    pub NOT_PLAYED =
        ko "아직 플레이하지 않음",
        en "아직 플레이하지 않음",
        ja "아직 플레이하지 않음";
    pub CLEAR_TO_RECORD =
        ko "이 채보를 클리어하면 기록이 남아요.",
        en "이 채보를 클리어하면 기록이 남아요.",
        ja "이 채보를 클리어하면 기록이 남아요.";
    /// `{}` is the gauge name (GROOVE, HARD, ...).
    pub GAUGE_NAME =
        ko "{} 게이지",
        en "{} 게이지",
        ja "{} 게이지";
    /// `{}` is the play count.
    pub PLAY_COUNT =
        ko "{}회 플레이",
        en "{}회 플레이",
        ja "{}회 플레이";
    pub ACCURACY =
        ko "정확도",
        en "정확도",
        ja "정확도";
    pub MAX_COMBO =
        ko "최대 콤보",
        en "최대 콤보",
        ja "최대 콤보";
    pub MIN_BP =
        ko "MIN BP",
        en "MIN BP",
        ja "MIN BP";
    pub PLAY =
        ko "플레이",
        en "플레이",
        ja "플레이";
    pub REPLAY =
        ko "리플레이",
        en "리플레이",
        ja "리플레이";
    pub AUTO_PLAY =
        ko "자동 플레이",
        en "자동 플레이",
        ja "자동 플레이";
    /// Shown after the folder's song count: `{}` is the count.
    pub SONGS_TOTAL =
        ko " / {} 곡",
        en " / {} 곡",
        ja " / {} 곡";
    pub BPM =
        ko "BPM",
        en "BPM",
        ja "BPM";
    pub NOTES =
        ko "노트",
        en "노트",
        ja "노트";
    pub MODE =
        ko "모드",
        en "모드",
        ja "모드";

    // Footer key captions (keycaps keep their English key names).
    pub FOOTER_MOVE =
        ko "이동",
        en "이동",
        ja "이동";
    pub FOOTER_PLAY =
        ko "플레이",
        en "플레이",
        ja "플레이";
    pub FOOTER_HELP =
        ko "도움말",
        en "도움말",
        ja "도움말";

    // Folder tree (song select). Mode tags (7K) and lamp names (PERFECT) stay in English.
    /// The root of the breadcrumb, above the top-level folders.
    pub FOLDER_ROOT =
        ko "전체",
        en "전체",
        ja "전체";
    pub FOLDER_ALL =
        ko "전체 곡",
        en "전체 곡",
        ja "전체 곡";
    pub FOLDER_MODE =
        ko "키 모드",
        en "키 모드",
        ja "키 모드";
    pub FOLDER_LEVEL =
        ko "레벨",
        en "레벨",
        ja "레벨";
    pub FOLDER_LAMP =
        ko "클리어 램프",
        en "클리어 램프",
        ja "클리어 램프";
    pub FOLDER_TABLE =
        ko "난이도표",
        en "난이도표",
        ja "난이도표";
    /// The first folder of 난이도표: every song in any table.
    pub FOLDER_TABLE_ALL =
        ko "전곡",
        en "전곡",
        ja "전곡";
    /// Detail panel and footer count of a folder: `{}` is the song count.
    pub FOLDER_SONGS =
        ko "{} 곡",
        en "{} 곡",
        ja "{} 곡";
    /// Footer count while the list is folders only: `{}` is the folder count.
    pub FOLDER_COUNT =
        ko "{} 폴더",
        en "{} 폴더",
        ja "{} 폴더";
    pub FOLDER_OPEN_HINT =
        ko "ENTER로 열고, BKSP로 위 폴더로 나가요.",
        en "ENTER로 열고, BKSP로 위 폴더로 나가요.",
        ja "ENTER로 열고, BKSP로 위 폴더로 나가요.";
    pub FOOTER_OPEN =
        ko "열기",
        en "열기",
        ja "열기";

    // Sort names.
    pub SORT_TITLE =
        ko "제목",
        en "제목",
        ja "제목";
    pub SORT_LEVEL =
        ko "레벨",
        en "레벨",
        ja "레벨";
    pub SORT_CLEAR_LAMP =
        ko "클리어",
        en "클리어",
        ja "클리어";
    pub SORT_SCORE_RATE =
        ko "정확도",
        en "정확도",
        ja "정확도";
    pub SORT_BPM =
        ko "BPM",
        en "BPM",
        ja "BPM";

    // Filter row and result count (song select, F10 focuses the row).
    pub FILTER =
        ko "필터",
        en "필터",
        ja "필터";
    pub FILTER_LEVEL =
        ko "레벨",
        en "레벨",
        ja "레벨";
    /// The level bounds while they are not set.
    pub FILTER_LEVEL_MIN_ANY =
        ko "최소",
        en "최소",
        ja "최소";
    pub FILTER_LEVEL_MAX_ANY =
        ko "최대",
        en "최대",
        ja "최대";
    pub FILTER_UNPLAYED =
        ko "미플레이만",
        en "미플레이만",
        ja "미플레이만";
    pub FILTER_UNCLEARED =
        ko "미클리어만",
        en "미클리어만",
        ja "미클리어만";
    pub FILTER_RESET =
        ko "초기화",
        en "초기화",
        ja "초기화";
    /// Empty list while a filter is on and there is no search.
    pub FILTER_NO_MATCH =
        ko "필터에 맞는 곡이 없어요.",
        en "필터에 맞는 곡이 없어요.",
        ja "필터에 맞는 곡이 없어요.";
    pub FILTER_EMPTY_HINT =
        ko "필터를 바꾸거나 초기화하세요.",
        en "필터를 바꾸거나 초기화하세요.",
        ja "필터를 바꾸거나 초기화하세요.";
    /// `{}` is the number of songs found.
    pub RESULT_COUNT =
        ko "{}곡 찾음",
        en "{}곡 찾음",
        ja "{}곡 찾음";

    // Help overlay (song select, ? key). Keys stay English keycaps.
    pub HELP_TITLE =
        ko "도움말",
        en "도움말",
        ja "도움말";
    pub HELP_CLOSE =
        ko "? 또는 ESC로 닫아요",
        en "? 또는 ESC로 닫아요",
        ja "? 또는 ESC로 닫아요";
    pub HELP_GROUP_MOVE =
        ko "이동",
        en "이동",
        ja "이동";
    pub HELP_GROUP_FOLDER =
        ko "폴더",
        en "폴더",
        ja "폴더";
    pub HELP_GROUP_SONG =
        ko "곡",
        en "곡",
        ja "곡";
    pub HELP_GROUP_OPTIONS =
        ko "옵션과 설정",
        en "옵션과 설정",
        ja "옵션과 설정";
    pub HELP_GROUP_FILTER =
        ko "필터와 정렬",
        en "필터와 정렬",
        ja "필터와 정렬";
    pub HELP_GROUP_MISC =
        ko "기타",
        en "기타",
        ja "기타";
    pub HELP_CAP_MOUSE =
        ko "마우스",
        en "마우스",
        ja "마우스";
    pub HELP_CAP_DROP =
        ko "끌어다 놓기",
        en "끌어다 놓기",
        ja "끌어다 놓기";
    pub HELP_MOVE_ONE =
        ko "한 줄씩 움직여요",
        en "한 줄씩 움직여요",
        ja "한 줄씩 움직여요";
    pub HELP_MOVE_PAGE =
        ko "10줄씩 움직여요",
        en "10줄씩 움직여요",
        ja "10줄씩 움직여요";
    pub HELP_MOVE_ENDS =
        ko "처음과 끝으로 가요",
        en "처음과 끝으로 가요",
        ja "처음과 끝으로 가요";
    pub HELP_MOUSE_WHEEL =
        ko "휠로 목록을 굴려요",
        en "휠로 목록을 굴려요",
        ja "휠로 목록을 굴려요";
    pub HELP_MOUSE_ROW =
        ko "줄을 누르면 골라요",
        en "줄을 누르면 골라요",
        ja "줄을 누르면 골라요";
    pub HELP_FOLDER_UP =
        ko "한 단계 위 폴더로 가요",
        en "한 단계 위 폴더로 가요",
        ja "한 단계 위 폴더로 가요";
    pub HELP_FOLDER_OPEN =
        ko "폴더는 열고 곡은 재생해요",
        en "폴더는 열고 곡은 재생해요",
        ja "폴더는 열고 곡은 재생해요";
    pub HELP_FOLDER_SIDE =
        ko "같은 단계의 이전과 다음 폴더",
        en "같은 단계의 이전과 다음 폴더",
        ja "같은 단계의 이전과 다음 폴더";
    pub HELP_MOUSE_SIDE =
        ko "상단 < > 버튼으로 옆 폴더",
        en "상단 < > 버튼으로 옆 폴더",
        ja "상단 < > 버튼으로 옆 폴더";
    pub HELP_MOUSE_CRUMB =
        ko "빵부스러기를 누르면 그 폴더",
        en "빵부스러기를 누르면 그 폴더",
        ja "빵부스러기를 누르면 그 폴더";
    pub HELP_SONG_PLAY =
        ko "선택한 곡을 플레이해요",
        en "선택한 곡을 플레이해요",
        ja "선택한 곡을 플레이해요";
    pub HELP_SONG_AUTO =
        ko "자동 플레이를 켜고 꺼요",
        en "자동 플레이를 켜고 꺼요",
        ja "자동 플레이를 켜고 꺼요";
    pub HELP_SONG_REPLAY =
        ko "저장된 리플레이를 재생해요",
        en "저장된 리플레이를 재생해요",
        ja "저장된 리플레이를 재생해요";
    pub HELP_SONG_CHART =
        ko "묶음 행의 채보를 바꿔요",
        en "묶음 행의 채보를 바꿔요",
        ja "묶음 행의 채보를 바꿔요";
    pub HELP_MOUSE_AGAIN =
        ko "고른 줄을 다시 누르면 재생",
        en "고른 줄을 다시 누르면 재생",
        ja "고른 줄을 다시 누르면 재생";
    pub HELP_OPTIONS =
        ko "플레이 옵션을 열어요",
        en "플레이 옵션을 열어요",
        ja "플레이 옵션을 열어요";
    pub HELP_SETTINGS =
        ko "설정 화면을 열어요",
        en "설정 화면을 열어요",
        ja "설정 화면을 열어요";
    pub HELP_KEYS =
        ko "키 설정을 열어요",
        en "키 설정을 열어요",
        ja "키 설정을 열어요";
    pub HELP_MOUSE_BUTTONS =
        ko "상단의 옵션, 설정 버튼",
        en "상단의 옵션, 설정 버튼",
        ja "상단의 옵션, 설정 버튼";
    pub HELP_SEARCH =
        ko "검색창을 열어요",
        en "검색창을 열어요",
        ja "검색창을 열어요";
    pub HELP_SORT =
        ko "정렬 기준을 바꿔요",
        en "정렬 기준을 바꿔요",
        ja "정렬 기준을 바꿔요";
    pub HELP_FILTER =
        ko "필터 줄에 초점을 줘요",
        en "필터 줄에 초점을 줘요",
        ja "필터 줄에 초점을 줘요";
    pub HELP_MOUSE_FILTER =
        ko "정렬, 필터 칩, 검색창 누르기",
        en "정렬, 필터 칩, 검색창 누르기",
        ja "정렬, 필터 칩, 검색창 누르기";
    pub HELP_BACK_QUIT =
        ko "한 단계 위, 루트에서는 종료",
        en "한 단계 위, 루트에서는 종료",
        ja "한 단계 위, 루트에서는 종료";
    pub HELP_RESCAN =
        ko "곡 목록을 다시 읽어요",
        en "곡 목록을 다시 읽어요",
        ja "곡 목록을 다시 읽어요";
    pub HELP_HELP =
        ko "도움말을 열고 닫아요",
        en "도움말을 열고 닫아요",
        ja "도움말을 열고 닫아요";
    pub HELP_DROP =
        ko "파일을 끌어다 놓으면 곡을 열어요",
        en "파일을 끌어다 놓으면 곡을 열어요",
        ja "파일을 끌어다 놓으면 곡을 열어요";

    // First run: the library has no songs. The guide lists three ways to add songs.
    pub GUIDE_TITLE =
        ko "아직 곡이 없어요",
        en "아직 곡이 없어요",
        ja "아직 곡이 없어요";
    pub GUIDE_LEAD =
        ko "곡을 넣는 방법은 세 가지예요.",
        en "곡을 넣는 방법은 세 가지예요.",
        ja "곡을 넣는 방법은 세 가지예요.";
    pub GUIDE_MANAGER =
        ko "곡 관리자로 곡 패키지(.bmsp)를 설치해요.",
        en "곡 관리자로 곡 패키지(.bmsp)를 설치해요.",
        ja "곡 관리자로 곡 패키지(.bmsp)를 설치해요.";
    pub GUIDE_DROP =
        ko "곡 파일(.bmsp, .bms)이나 폴더를 창에 끌어다 놓아요.",
        en "곡 파일(.bmsp, .bms)이나 폴더를 창에 끌어다 놓아요.",
        ja "곡 파일(.bmsp, .bms)이나 폴더를 창에 끌어다 놓아요.";
    pub GUIDE_FOLDER =
        ko "곡 파일을 songs 폴더에 넣어요.",
        en "곡 파일을 songs 폴더에 넣어요.",
        ja "곡 파일을 songs 폴더에 넣어요.";
    pub GUIDE_BTN_MANAGER =
        ko "곡 관리자 열기",
        en "곡 관리자 열기",
        ja "곡 관리자 열기";
    pub GUIDE_BTN_FOLDER =
        ko "songs 폴더 열기",
        en "songs 폴더 열기",
        ja "songs 폴더 열기";

    // Drag overlay: a file is dragged over the window (song select).
    pub DROP_TITLE =
        ko "여기에 놓으면 곡을 열어요",
        en "여기에 놓으면 곡을 열어요",
        ja "여기에 놓으면 곡을 열어요";
    pub DROP_NOTE =
        ko ".bmsp 패키지나 .bms 채보를 놓으면 바로 열어요.",
        en ".bmsp 패키지나 .bms 채보를 놓으면 바로 열어요.",
        ja ".bmsp 패키지나 .bms 채보를 놓으면 바로 열어요.";

    // Opening the song manager or the songs folder failed.
    pub TOAST_MANAGER_OPENED =
        ko "곡을 넣은 뒤 이 창으로 돌아오면 목록이 갱신돼요",
        en "곡을 넣은 뒤 이 창으로 돌아오면 목록이 갱신돼요",
        ja "곡을 넣은 뒤 이 창으로 돌아오면 목록이 갱신돼요";
    pub TOAST_MANAGER_MISSING =
        ko "곡 관리자(bpm-gui.exe)를 찾지 못했어요",
        en "곡 관리자(bpm-gui.exe)를 찾지 못했어요",
        ja "곡 관리자(bpm-gui.exe)를 찾지 못했어요";
    pub TOAST_FOLDER_FAILED =
        ko "songs 폴더를 열지 못했어요",
        en "songs 폴더를 열지 못했어요",
        ja "songs 폴더를 열지 못했어요";

    // Play options panel (song select, TAB / O): per-play values only.
    pub MODAL_PLAY_OPTIONS =
        ko "플레이 옵션",
        en "플레이 옵션",
        ja "플레이 옵션";
    pub GROUP_PLAY =
        ko "플레이",
        en "플레이",
        ja "플레이";
    pub GROUP_SESSION =
        ko "세션",
        en "세션",
        ja "세션";
    pub GROUP_DISPLAY =
        ko "화면",
        en "화면",
        ja "화면";
    pub GROUP_AUDIO =
        ko "소리",
        en "소리",
        ja "소리";
    pub GROUP_JUDGE =
        ko "판정",
        en "판정",
        ja "판정";
    pub GROUP_LAYOUT =
        ko "레이아웃",
        en "레이아웃",
        ja "레이아웃";
    pub GROUP_INPUT =
        ko "입력",
        en "입력",
        ja "입력";
    pub ROW_GREEN =
        ko "그린 넘버",
        en "그린 넘버",
        ja "그린 넘버";
    pub ROW_LANE_COVER =
        ko "레인 커버",
        en "레인 커버",
        ja "레인 커버";
    pub ROW_MODIFIER =
        ko "모디파이어",
        en "모디파이어",
        ja "모디파이어";
    pub ROW_GAUGE =
        ko "게이지",
        en "게이지",
        ja "게이지";
    pub ROW_LN_MODE =
        ko "LN 모드",
        en "LN 모드",
        ja "LN 모드";
    pub ROW_AUTO_PLAY =
        ko "자동 플레이",
        en "자동 플레이",
        ja "자동 플레이";
    pub ROW_START_MEASURE =
        ko "시작 마디",
        en "시작 마디",
        ja "시작 마디";
    pub ROW_JUDGE_OFFSET =
        ko "판정 오프셋",
        en "판정 오프셋",
        ja "판정 오프셋";
    pub ROW_MASTER_VOLUME =
        ko "전체 볼륨",
        en "전체 볼륨",
        ja "전체 볼륨";
    pub ROW_BGM_VOLUME =
        ko "배경음 볼륨",
        en "배경음 볼륨",
        ja "배경음 볼륨";
    pub ROW_KEY_VOLUME =
        ko "키음 볼륨",
        en "키음 볼륨",
        ja "키음 볼륨";
    pub ROW_SFX_VOLUME =
        ko "효과음 볼륨",
        en "효과음 볼륨",
        ja "효과음 볼륨";
    pub ROW_VISUAL_OFFSET =
        ko "비주얼 오프셋",
        en "비주얼 오프셋",
        ja "비주얼 오프셋";
    pub ROW_JUDGE_POSITION =
        ko "판정 표시 위치",
        en "판정 표시 위치",
        ja "판정 표시 위치";
    pub JUDGE_POS_UPPER =
        ko "위",
        en "위",
        ja "위";
    pub JUDGE_POS_MIDDLE =
        ko "가운데",
        en "가운데",
        ja "가운데";
    pub JUDGE_POS_LOWER =
        ko "아래",
        en "아래",
        ja "아래";
    pub ROW_NOTE_SIZE =
        ko "노트 크기",
        en "노트 크기",
        ja "노트 크기";
    pub ROW_REDUCE_FLASHES =
        ko "플래시 줄이기",
        en "플래시 줄이기",
        ja "플래시 줄이기";
    pub ROW_PLAYFIELD =
        ko "플레이필드",
        en "플레이필드",
        ja "플레이필드";
    pub ROW_SCRATCH =
        ko "스크래치",
        en "스크래치",
        ja "스크래치";
    pub ROW_BGA =
        ko "BGA",
        en "BGA",
        ja "BGA";
    pub ROW_TRACK_BGA =
        ko "트랙 BGA",
        en "트랙 BGA",
        ja "트랙 BGA";
    pub ROW_KEY_HINT =
        ko "플레이 중 키 안내",
        en "플레이 중 키 안내",
        ja "플레이 중 키 안내";
    pub KEY_HINT_FIRST =
        ko "처음 3초만",
        en "처음 3초만",
        ja "처음 3초만";
    pub KEY_HINT_ALWAYS =
        ko "항상",
        en "항상",
        ja "항상";
    pub KEY_HINT_OFF =
        ko "끄기",
        en "끄기",
        ja "끄기";
    pub ROW_DISPLAY_MODE =
        ko "화면 모드",
        en "화면 모드",
        ja "화면 모드";
    pub ROW_RESOLUTION =
        ko "해상도",
        en "해상도",
        ja "해상도";
    pub ROW_GRAPHICS =
        ko "그래픽",
        en "그래픽",
        ja "그래픽";
    pub ROW_TARGET_FPS =
        ko "목표 FPS",
        en "목표 FPS",
        ja "목표 FPS";
    pub ROW_KEY_LAYOUT =
        ko "키 배치",
        en "키 배치",
        ja "키 배치";
    pub VALUE_ON =
        ko "켜짐",
        en "켜짐",
        ja "켜짐";
    pub VALUE_OFF =
        ko "꺼짐",
        en "꺼짐",
        ja "꺼짐";
    pub VALUE_UNLIMITED =
        ko "무제한",
        en "무제한",
        ja "무제한";
    pub VALUE_CENTER =
        ko "가운데",
        en "가운데",
        ja "가운데";
    /// A row that does not apply to the selected song's key mode.
    pub VALUE_NOT_APPLICABLE =
        ko "해당 없음",
        en "해당 없음",
        ja "해당 없음";
    /// Resolution label when the window size matches no preset.
    pub RESOLUTION_CUSTOM =
        ko "사용자 지정",
        en "사용자 지정",
        ja "사용자 지정";
    pub DISPLAY_WINDOWED =
        ko "창 모드",
        en "창 모드",
        ja "창 모드";
    pub DISPLAY_BORDERLESS =
        ko "테두리 없는 창",
        en "테두리 없는 창",
        ja "테두리 없는 창";
    pub DISPLAY_FULLSCREEN =
        ko "전체 화면",
        en "전체 화면",
        ja "전체 화면";
    pub GPU_AUTO =
        ko "자동",
        en "자동",
        ja "자동";
    /// Backend name, kept in English.
    pub GPU_WARP =
        ko "WARP (CPU)",
        en "WARP (CPU)",
        ja "WARP (CPU)";
    pub TRACK_BGA_OFF =
        ko "꺼짐 (0%)",
        en "꺼짐 (0%)",
        ja "꺼짐 (0%)";
    pub TRACK_BGA_LOW =
        ko "낮음 (25%)",
        en "낮음 (25%)",
        ja "낮음 (25%)";
    pub TRACK_BGA_MEDIUM =
        ko "보통 (50%)",
        en "보통 (50%)",
        ja "보통 (50%)";
    pub TRACK_BGA_HIGH =
        ko "높음 (75%)",
        en "높음 (75%)",
        ja "높음 (75%)";
    /// `{}` is the graphics backend; the note follows it.
    pub VALUE_AFTER_RESTART =
        ko "{} (재시작 후 적용)",
        en "{} (재시작 후 적용)",
        ja "{} (재시작 후 적용)";
    /// `{}` is the FPS number.
    pub VALUE_FPS =
        ko "{} FPS",
        en "{} FPS",
        ja "{} FPS";
    /// `{}` is the LN rule AUTO resolves to for the highlighted song.
    pub VALUE_AUTO_RESOLVED =
        ko "자동 ({})",
        en "자동 ({})",
        ja "자동 ({})";
    /// `{}` is the green number in milliseconds.
    pub VALUE_MS =
        ko "{} ms",
        en "{} ms",
        ja "{} ms";
    /// Option chip above PLAY; `{}` is the green number in milliseconds.
    pub CHIP_GREEN =
        ko "G {}",
        en "G {}",
        ja "G {}";
    /// Modal footer hints.
    pub HINT_MOVE =
        ko "이동",
        en "이동",
        ja "이동";
    pub HINT_CHANGE =
        ko "변경",
        en "변경",
        ja "변경";
    pub HINT_CLOSE =
        ko "닫기",
        en "닫기",
        ja "닫기";
    pub HINT_START =
        ko "시작",
        en "시작",
        ja "시작";
    pub HINT_SELECT =
        ko "선택",
        en "선택",
        ja "선택";

    // Help sentences: one line under the highlighted row (plain Korean).
    pub HELP_GREEN =
        ko "노트가 화면에 나타나서 판정선에 닿기까지의 시간이에요. 작을수록 빨라요. 레인 커버를 바꿔도 유지돼요.",
        en "노트가 화면에 나타나서 판정선에 닿기까지의 시간이에요. 작을수록 빨라요. 레인 커버를 바꿔도 유지돼요.",
        ja "노트가 화면에 나타나서 판정선에 닿기까지의 시간이에요. 작을수록 빨라요. 레인 커버를 바꿔도 유지돼요.";
    pub HELP_LANE_COVER =
        ko "레인 위쪽을 가려 노트가 보이는 구간을 줄여요. 0%는 가리지 않아요. 플레이 중 F10, F11로도 바꿀 수 있어요.",
        en "레인 위쪽을 가려 노트가 보이는 구간을 줄여요. 0%는 가리지 않아요. 플레이 중 F10, F11로도 바꿀 수 있어요.",
        ja "레인 위쪽을 가려 노트가 보이는 구간을 줄여요. 0%는 가리지 않아요. 플레이 중 F10, F11로도 바꿀 수 있어요.";
    pub HELP_MODIFIER =
        ko "노트 배치를 바꿔요. 미러는 좌우를 뒤집고, 랜덤 계열은 레인 순서를 섞어요.",
        en "노트 배치를 바꿔요. 미러는 좌우를 뒤집고, 랜덤 계열은 레인 순서를 섞어요.",
        ja "노트 배치를 바꿔요. 미러는 좌우를 뒤집고, 랜덤 계열은 레인 순서를 섞어요.";
    pub HELP_GAUGE =
        ko "점수 게이지의 방식이에요. EASY와 GROOVE는 관대하고, HARD와 HAZARD는 실수에 엄격해요.",
        en "점수 게이지의 방식이에요. EASY와 GROOVE는 관대하고, HARD와 HAZARD는 실수에 엄격해요.",
        ja "점수 게이지의 방식이에요. EASY와 GROOVE는 관대하고, HARD와 HAZARD는 실수에 엄격해요.";
    pub HELP_LN_MODE =
        ko "롱노트 판정 방식이에요. AUTO는 곡 데이터에 맞추고, LN과 CN은 어떤 곡이든 그 방식으로 고정해요.",
        en "롱노트 판정 방식이에요. AUTO는 곡 데이터에 맞추고, LN과 CN은 어떤 곡이든 그 방식으로 고정해요.",
        ja "롱노트 판정 방식이에요. AUTO는 곡 데이터에 맞추고, LN과 CN은 어떤 곡이든 그 방식으로 고정해요.";
    pub HELP_AUTO_PLAY =
        ko "켜면 키를 누르지 않아도 곡이 끝까지 연주돼요. 자동 플레이 결과는 기록으로 저장되지 않아요.",
        en "켜면 키를 누르지 않아도 곡이 끝까지 연주돼요. 자동 플레이 결과는 기록으로 저장되지 않아요.",
        ja "켜면 키를 누르지 않아도 곡이 끝까지 연주돼요. 자동 플레이 결과는 기록으로 저장되지 않아요.";
    pub HELP_START_MEASURE =
        ko "이 마디부터 곡을 시작해요. 0이면 처음부터예요. 0이 아니면 연습으로 기록되지 않아요.",
        en "이 마디부터 곡을 시작해요. 0이면 처음부터예요. 0이 아니면 연습으로 기록되지 않아요.",
        ja "이 마디부터 곡을 시작해요. 0이면 처음부터예요. 0이 아니면 연습으로 기록되지 않아요.";

    // Settings screen (its title is SETTINGS above).
    pub HELP_DISPLAY_MODE =
        ko "창 모드, 테두리 없는 창, 전체 화면 중에서 골라요. 바로 적용돼요.",
        en "창 모드, 테두리 없는 창, 전체 화면 중에서 골라요. 바로 적용돼요.",
        ja "창 모드, 테두리 없는 창, 전체 화면 중에서 골라요. 바로 적용돼요.";
    pub HELP_RESOLUTION =
        ko "창 크기나 전체 화면 해상도를 골라요. 창 모드는 16:9 크기만 고를 수 있어요.",
        en "창 크기나 전체 화면 해상도를 골라요. 창 모드는 16:9 크기만 고를 수 있어요.",
        ja "창 크기나 전체 화면 해상도를 골라요. 창 모드는 16:9 크기만 고를 수 있어요.";
    pub HELP_GRAPHICS =
        ko "그림을 그리는 장치예요. 자동은 그래픽 카드를 쓰고, WARP는 CPU로 그려요. 바꾸면 재시작 후 적용돼요.",
        en "그림을 그리는 장치예요. 자동은 그래픽 카드를 쓰고, WARP는 CPU로 그려요. 바꾸면 재시작 후 적용돼요.",
        ja "그림을 그리는 장치예요. 자동은 그래픽 카드를 쓰고, WARP는 CPU로 그려요. 바꾸면 재시작 후 적용돼요.";
    pub HELP_TARGET_FPS =
        ko "플레이 화면의 최대 프레임이에요. 무제한은 제한하지 않고, 60은 화면 주사율에 맞춰요.",
        en "플레이 화면의 최대 프레임이에요. 무제한은 제한하지 않고, 60은 화면 주사율에 맞춰요.",
        ja "플레이 화면의 최대 프레임이에요. 무제한은 제한하지 않고, 60은 화면 주사율에 맞춰요.";
    pub HELP_MASTER_VOLUME =
        ko "모든 소리의 전체 크기예요. 아래 세 볼륨에 곱해지며, 최대 200%까지 올릴 수 있어요.",
        en "모든 소리의 전체 크기예요. 아래 세 볼륨에 곱해지며, 최대 200%까지 올릴 수 있어요.",
        ja "모든 소리의 전체 크기예요. 아래 세 볼륨에 곱해지며, 최대 200%까지 올릴 수 있어요.";
    pub HELP_BGM_VOLUME =
        ko "곡의 배경음 크기예요. 채보에 들어 있는 자동 재생 소리가 여기에 해당해요. 전체 볼륨과 함께 적용돼요.",
        en "곡의 배경음 크기예요. 채보에 들어 있는 자동 재생 소리가 여기에 해당해요. 전체 볼륨과 함께 적용돼요.",
        ja "곡의 배경음 크기예요. 채보에 들어 있는 자동 재생 소리가 여기에 해당해요. 전체 볼륨과 함께 적용돼요.";
    pub HELP_KEY_VOLUME =
        ko "플레이 중 내가 누른 키의 소리 크기예요. 판정된 노트의 소리도 여기에 해당해요. 자동 플레이 소리는 배경음이에요.",
        en "플레이 중 내가 누른 키의 소리 크기예요. 판정된 노트의 소리도 여기에 해당해요. 자동 플레이 소리는 배경음이에요.",
        ja "플레이 중 내가 누른 키의 소리 크기예요. 판정된 노트의 소리도 여기에 해당해요. 자동 플레이 소리는 배경음이에요.";
    pub HELP_SFX_VOLUME =
        ko "메뉴 효과음과 징글의 크기예요. 곡 미리듣기는 배경음 볼륨을 따라요.",
        en "메뉴 효과음과 징글의 크기예요. 곡 미리듣기는 배경음 볼륨을 따라요.",
        ja "메뉴 효과음과 징글의 크기예요. 곡 미리듣기는 배경음 볼륨을 따라요.";
    pub HELP_JUDGE_OFFSET =
        ko "판정 시각을 앞뒤로 옮겨서 판정 결과가 바뀌어요. 범위는 -100에서 +100 ms예요. 화면만 옮기려면 비주얼 오프셋을 써요.",
        en "판정 시각을 앞뒤로 옮겨서 판정 결과가 바뀌어요. 범위는 -100에서 +100 ms예요. 화면만 옮기려면 비주얼 오프셋을 써요.",
        ja "판정 시각을 앞뒤로 옮겨서 판정 결과가 바뀌어요. 범위는 -100에서 +100 ms예요. 화면만 옮기려면 비주얼 오프셋을 써요.";
    pub HELP_VISUAL_OFFSET =
        ko "노트를 그리는 위치만 앞뒤로 옮겨요. 양수면 노트가 더 일찍 보여요. 판정과 소리 타이밍은 바뀌지 않아요.",
        en "노트를 그리는 위치만 앞뒤로 옮겨요. 양수면 노트가 더 일찍 보여요. 판정과 소리 타이밍은 바뀌지 않아요.",
        ja "노트를 그리는 위치만 앞뒤로 옮겨요. 양수면 노트가 더 일찍 보여요. 판정과 소리 타이밍은 바뀌지 않아요.";
    pub HELP_JUDGE_POSITION =
        ko "판정과 콤보 글자가 판정선에서 얼마나 떨어져 보일지 정해요. 위가 기본이에요.",
        en "판정과 콤보 글자가 판정선에서 얼마나 떨어져 보일지 정해요. 위가 기본이에요.",
        ja "판정과 콤보 글자가 판정선에서 얼마나 떨어져 보일지 정해요. 위가 기본이에요.";
    pub HELP_NOTE_SIZE =
        ko "싱글 플레이의 레인 너비를 90%에서 120%까지 바꿔요. 더블 플레이는 그대로예요.",
        en "싱글 플레이의 레인 너비를 90%에서 120%까지 바꿔요. 더블 플레이는 그대로예요.",
        ja "싱글 플레이의 레인 너비를 90%에서 120%까지 바꿔요. 더블 플레이는 그대로예요.";
    pub HELP_REDUCE_FLASHES =
        ko "빨간 경고 번쩍임, 판정 불빛, 배너의 빛 줄기와 흔들림을 줄여요. 판정과 소리는 그대로예요.",
        en "빨간 경고 번쩍임, 판정 불빛, 배너의 빛 줄기와 흔들림을 줄여요. 판정과 소리는 그대로예요.",
        ja "빨간 경고 번쩍임, 판정 불빛, 배너의 빛 줄기와 흔들림을 줄여요. 판정과 소리는 그대로예요.";
    pub HELP_PLAYFIELD =
        ko "플레이필드를 화면 가운데, 왼쪽, 오른쪽 중 어디에 둘지 정해요.",
        en "플레이필드를 화면 가운데, 왼쪽, 오른쪽 중 어디에 둘지 정해요.",
        ja "플레이필드를 화면 가운데, 왼쪽, 오른쪽 중 어디에 둘지 정해요.";
    pub HELP_SCRATCH =
        ko "스크래치 레인을 왼쪽이나 오른쪽 가장자리에 둬요. 5K, 7K, 8K에서만 적용돼요.",
        en "스크래치 레인을 왼쪽이나 오른쪽 가장자리에 둬요. 5K, 7K, 8K에서만 적용돼요.",
        ja "스크래치 레인을 왼쪽이나 오른쪽 가장자리에 둬요. 5K, 7K, 8K에서만 적용돼요.";
    pub HELP_BGA =
        ko "곡의 배경 영상과 이미지를 보여 줘요. 끄면 이 파일들을 읽지 않아 메모리와 CPU를 아껴요.",
        en "곡의 배경 영상과 이미지를 보여 줘요. 끄면 이 파일들을 읽지 않아 메모리와 CPU를 아껴요.",
        ja "곡의 배경 영상과 이미지를 보여 줘요. 끄면 이 파일들을 읽지 않아 메모리와 CPU를 아껴요.";
    pub HELP_KEY_HINT =
        ko "플레이 화면 아래의 키 안내 줄을 정해요. 처음 3초만이면 시작 뒤 3초 동안만 보이고 사라져요.",
        en "플레이 화면 아래의 키 안내 줄을 정해요. 처음 3초만이면 시작 뒤 3초 동안만 보이고 사라져요.",
        ja "플레이 화면 아래의 키 안내 줄을 정해요. 처음 3초만이면 시작 뒤 3초 동안만 보이고 사라져요.";
    pub HELP_TRACK_BGA =
        ko "배경 영상과 이미지를 화면 배경에 얼마나 진하게 비출지 정해요. 꺼짐이면 보이지 않아요.",
        en "배경 영상과 이미지를 화면 배경에 얼마나 진하게 비출지 정해요. 꺼짐이면 보이지 않아요.",
        ja "배경 영상과 이미지를 화면 배경에 얼마나 진하게 비출지 정해요. 꺼짐이면 보이지 않아요.";
    pub HELP_KEY_LAYOUT =
        ko "선택한 곡의 키 모드에서 쓸 키 배치를 골라요. ENTER로 키 설정 화면을 열어요.",
        en "선택한 곡의 키 모드에서 쓸 키 배치를 골라요. ENTER로 키 설정 화면을 열어요.",
        ja "선택한 곡의 키 모드에서 쓸 키 배치를 골라요. ENTER로 키 설정 화면을 열어요.";

    // Judge offset calibration (a Settings sub-screen).
    pub CALIBRATE_TITLE =
        ko "판정 오프셋 측정",
        en "판정 오프셋 측정",
        ja "판정 오프셋 측정";
    pub CALIBRATE_INSTRUCTION =
        ko "소리에 맞춰 스페이스를 누르세요",
        en "소리에 맞춰 스페이스를 누르세요",
        ja "소리에 맞춰 스페이스를 누르세요";
    pub CALIBRATE_DONE_TITLE =
        ko "측정이 끝났어요",
        en "측정이 끝났어요",
        ja "측정이 끝났어요";
    pub CALIBRATE_NO_AUDIO =
        ko "오디오 출력 장치를 열지 못해 측정할 수 없어요",
        en "오디오 출력 장치를 열지 못해 측정할 수 없어요",
        ja "오디오 출력 장치를 열지 못해 측정할 수 없어요";
    pub CALIBRATE_COUNT_IN =
        ko "준비",
        en "준비",
        ja "준비";
    pub CALIBRATE_MEASURING =
        ko "측정 중",
        en "측정 중",
        ja "측정 중";
    pub CALIBRATE_PROGRESS =
        ko "진행",
        en "진행",
        ja "진행";
    pub CALIBRATE_MEAN =
        ko "평균",
        en "평균",
        ja "평균";
    pub CALIBRATE_SPREAD =
        ko "편차",
        en "편차",
        ja "편차";
    pub CALIBRATE_SUGGEST =
        ko "제안값",
        en "제안값",
        ja "제안값";
    pub CALIBRATE_EARLY =
        ko "빠름",
        en "빠름",
        ja "빠름";
    pub CALIBRATE_LATE =
        ko "늦음",
        en "늦음",
        ja "늦음";
    pub CALIBRATE_EXCLUDED =
        ko "제외된 입력은 흐리게 표시해요",
        en "제외된 입력은 흐리게 표시해요",
        ja "제외된 입력은 흐리게 표시해요";
    pub CALIBRATE_HELP =
        ko "준비 박 네 개 뒤부터 입력을 16번 세어요. 한 박에 한 번만 세고, 150 ms보다 멀리 떨어진 입력은 무시해요.",
        en "준비 박 네 개 뒤부터 입력을 16번 세어요. 한 박에 한 번만 세고, 150 ms보다 멀리 떨어진 입력은 무시해요.",
        ja "준비 박 네 개 뒤부터 입력을 16번 세어요. 한 박에 한 번만 세고, 150 ms보다 멀리 떨어진 입력은 무시해요.";
    pub CALIBRATE_HELP_DONE =
        ko "제안값은 평균 차이의 반대 값이에요. 적용하면 판정 오프셋이 이 값이 돼요.",
        en "제안값은 평균 차이의 반대 값이에요. 적용하면 판정 오프셋이 이 값이 돼요.",
        ja "제안값은 평균 차이의 반대 값이에요. 적용하면 판정 오프셋이 이 값이 돼요.";
    pub CALIBRATE_HELP_NO_AUDIO =
        ko "설정은 바뀌지 않았어요. ESC로 설정 화면에 돌아가세요.",
        en "설정은 바뀌지 않았어요. ESC로 설정 화면에 돌아가세요.",
        ja "설정은 바뀌지 않았어요. ESC로 설정 화면에 돌아가세요.";
    pub CALIBRATE_APPLY =
        ko "적용",
        en "적용",
        ja "적용";
    pub CALIBRATE_RETRY =
        ko "다시",
        en "다시",
        ja "다시";
    pub CALIBRATE_CANCEL =
        ko "취소",
        en "취소",
        ja "취소";
    pub CALIBRATE_HINT_TAP =
        ko "누르기",
        en "누르기",
        ja "누르기";
    pub TOAST_CALIBRATE_APPLIED =
        ko "판정 오프셋을 {} ms로 맞췄어요",
        en "판정 오프셋을 {} ms로 맞췄어요",
        ja "판정 오프셋을 {} ms로 맞췄어요";

    // Quit confirmation.
    pub QUIT_TITLE =
        ko "BEETLE을 종료할까요?",
        en "BEETLE을 종료할까요?",
        ja "BEETLE을 종료할까요?";
    pub QUIT_NOTE =
        ko "기록과 설정은 이미 저장되어 있어요.",
        en "기록과 설정은 이미 저장되어 있어요.",
        ja "기록과 설정은 이미 저장되어 있어요.";
    pub QUIT =
        ko "종료",
        en "종료",
        ja "종료";

    // ---- Loading ------------------------------------------------------
    pub LOADING_STATUS =
        ko "키음을 디코딩하고 오디오를 준비하는 중",
        en "키음을 디코딩하고 오디오를 준비하는 중",
        ja "키음을 디코딩하고 오디오를 준비하는 중";
    pub PRACTICE =
        ko "연습",
        en "연습",
        ja "연습";

    // ---- Gameplay -----------------------------------------------------
    pub COMBO =
        ko "콤보",
        en "콤보",
        ja "콤보";
    pub NO_BGA =
        ko "BGA 없음",
        en "BGA 없음",
        ja "BGA 없음";
    pub PAUSED =
        ko "일시정지",
        en "일시정지",
        ja "일시정지";
    pub PAUSE_RESUME =
        ko "계속하기",
        en "계속하기",
        ja "계속하기";
    pub PAUSE_RESTART =
        ko "처음부터",
        en "처음부터",
        ja "처음부터";
    pub PAUSE_QUIT =
        ko "곡 선택으로",
        en "곡 선택으로",
        ja "곡 선택으로";
    pub PAUSE_HINT =
        ko "위아래 이동   ENTER 확인   R 처음부터",
        en "위아래 이동   ENTER 확인   R 처음부터",
        ja "위아래 이동   ENTER 확인   R 처음부터";
    /// Key hint line during a replay or auto play.
    pub HUD_BACK_TO_SELECT =
        ko "ESC  곡 선택으로",
        en "ESC  곡 선택으로",
        ja "ESC  곡 선택으로";
    /// Key hint line while playing; `{}` is the key layout.
    pub HUD_KEYS =
        ko "키  {}    1/2 그린 넘버    F10/F11 커버    ESC 일시정지",
        en "키  {}    1/2 그린 넘버    F10/F11 커버    ESC 일시정지",
        ja "키  {}    1/2 그린 넘버    F10/F11 커버    ESC 일시정지";
    /// Same, for layouts without cover keys (double play).
    pub HUD_KEYS_NO_COVER =
        ko "키  {}    1/2 그린 넘버    ESC 일시정지",
        en "키  {}    1/2 그린 넘버    ESC 일시정지",
        ja "키  {}    1/2 그린 넘버    ESC 일시정지";
    pub KEYS_CUSTOM =
        ko "사용자 배치",
        en "사용자 배치",
        ja "사용자 배치";
    /// Shown from the song start until a second before the first note.
    pub READY =
        ko "READY",
        en "READY",
        ja "READY";
    /// Flashes on the first note (after the count-in, or after READY).
    pub GO =
        ko "GO!",
        en "GO!",
        ja "GO!";
    pub READY_HINT =
        ko "F3/F4 그린 넘버 / F10/F11 레인 커버",
        en "F3/F4 그린 넘버 / F10/F11 레인 커버",
        ja "F3/F4 그린 넘버 / F10/F11 레인 커버";
    /// Readout over the lane after a green number change (`{}` = ms).
    pub READOUT_GREEN =
        ko "{} ms",
        en "{} ms",
        ja "{} ms";
    /// Readout over the lane after a lane cover change (`{}` = percent).
    pub READOUT_COVER =
        ko "커버 {}%",
        en "커버 {}%",
        ja "커버 {}%";
    pub TIMELINE_TITLE =
        ko "판정 타임라인",
        en "판정 타임라인",
        ja "판정 타임라인";
    pub TIMELINE_RANGE =
        ko "최근 8초",
        en "최근 8초",
        ja "최근 8초";
    pub TIMING_FAST =
        ko "FAST",
        en "FAST",
        ja "FAST";
    pub TIMING_SLOW =
        ko "SLOW",
        en "SLOW",
        ja "SLOW";
    /// Label before the pacemaker difference (EX score against the AAA pace).
    pub PACE =
        ko "PACE",
        en "PACE",
        ja "PACE";
    /// The next rank and the EX score still needed for it; `{}` = "AA +123".
    pub NEXT_RANK =
        ko "NEXT {}",
        en "NEXT {}",
        ja "NEXT {}";

    // ---- Result -------------------------------------------------------
    pub RESULT =
        ko "결과",
        en "결과",
        ja "결과";
    pub RESULT_SONG_SELECT =
        ko "곡 선택",
        en "곡 선택",
        ja "곡 선택";
    pub RESULT_RETRY =
        ko "다시 하기",
        en "다시 하기",
        ja "다시 하기";
    pub RESULT_RETRY_OPTIONS =
        ko "옵션 바꿔 다시 하기",
        en "옵션 바꿔 다시 하기",
        ja "옵션 바꿔 다시 하기";
    pub RESULT_SCREENSHOT =
        ko "스크린샷",
        en "스크린샷",
        ja "스크린샷";
    pub SCORE_NOT_SAVED =
        ko "기록이 저장되지 않음",
        en "기록이 저장되지 않음",
        ja "기록이 저장되지 않음";
    /// The button of the play options panel opened from the result: play
    /// again with the options as changed (ENTER).
    pub RESULT_OPTIONS_START =
        ko "새 옵션으로 다시 하기",
        en "새 옵션으로 다시 하기",
        ja "새 옵션으로 다시 하기";
    /// Trend graph: the clear line (`{}` = gauge percent) and the fail line of
    /// the HARD / HAZARD gauges, which fail at 0.
    pub GRAPH_CLEAR_LINE =
        ko "클리어 {}%",
        en "클리어 {}%",
        ja "클리어 {}%";
    pub GRAPH_FAIL_LINE =
        ko "실패 0%",
        en "실패 0%",
        ja "실패 0%";
    /// Marks the point where the stage failed on the trend graph.
    pub GRAPH_FAILED =
        ko "FAILED",
        en "FAILED",
        ja "FAILED";
    /// Tag next to the clear lamp when the lamp got better.
    pub NEW_LAMP =
        ko "램프 갱신",
        en "램프 갱신",
        ja "램프 갱신";
    /// Clear banners. Kept in English next to the clear lamps (FULL COMBO, FAILED).
    pub STAGE_CLEAR =
        ko "STAGE CLEAR",
        en "STAGE CLEAR",
        ja "STAGE CLEAR";
    pub STAGE_FAILED =
        ko "STAGE FAILED",
        en "STAGE FAILED",
        ja "STAGE FAILED";
    pub PERFECT =
        ko "PERFECT",
        en "PERFECT",
        ja "PERFECT";
    pub FULL_COMBO =
        ko "FULL COMBO",
        en "FULL COMBO",
        ja "FULL COMBO";
    pub NEW_RECORD =
        ko "신기록",
        en "신기록",
        ja "신기록";
    /// `{}` is the gauge name; `{}` is the rule note (e.g. "LN").
    pub GAUGE_NAME_RULE =
        ko "{} 게이지 / {}",
        en "{} 게이지 / {}",
        ja "{} 게이지 / {}";
    /// `{}` is the best EX score.
    pub BEST_EX =
        ko "최고 {}",
        en "최고 {}",
        ja "최고 {}";
    pub FIRST_PLAY =
        ko "첫 플레이",
        en "첫 플레이",
        ja "첫 플레이";
    pub JUDGE =
        ko "판정",
        en "판정",
        ja "판정";
    pub TIMING =
        ko "타이밍",
        en "타이밍",
        ja "타이밍";
    pub OFFSET =
        ko "오프셋",
        en "오프셋",
        ja "오프셋";
    pub MISS_COUNT =
        ko "MISS 수",
        en "MISS 수",
        ja "MISS 수";

    // ---- Key config ---------------------------------------------------
    pub KEY_CONFIG =
        ko "키 설정",
        en "키 설정",
        ja "키 설정";
    pub KEY_LANE =
        ko "레인",
        en "레인",
        ja "레인";
    pub SET_KEY =
        ko "키 지정",
        en "키 지정",
        ja "키 지정";
    pub ADD_KEY =
        ko "키 추가",
        en "키 추가",
        ja "키 추가";
    pub CLEAR_LANE =
        ko "비우기",
        en "비우기",
        ja "비우기";
    pub PRESET =
        ko "프리셋",
        en "프리셋",
        ja "프리셋";
    pub SCRATCH =
        ko "스크래치",
        en "스크래치",
        ja "스크래치";
    pub FORM_8K =
        ko "8K 형태",
        en "8K 형태",
        ja "8K 형태";
    pub RESET =
        ko "초기화",
        en "초기화",
        ja "초기화";
    pub ANY_KEY =
        ko "아무 키",
        en "아무 키",
        ja "아무 키";
    pub BIND =
        ko "지정",
        en "지정",
        ja "지정";
    pub SELECTED_LANE =
        ko "선택한 레인",
        en "선택한 레인",
        ja "선택한 레인";
    pub KEYS_CAPTION =
        ko "키",
        en "키",
        ja "키";
    pub SIDE_LEFT =
        ko "왼쪽",
        en "왼쪽",
        ja "왼쪽";
    pub SIDE_RIGHT =
        ko "오른쪽",
        en "오른쪽",
        ja "오른쪽";
    pub LAYOUT_TRIGGERS =
        ko "왼쪽, 오른쪽 트리거 사이에 키 6개 (F3: 일자 배치)",
        en "왼쪽, 오른쪽 트리거 사이에 키 6개 (F3: 일자 배치)",
        ja "왼쪽, 오른쪽 트리거 사이에 키 6개 (F3: 일자 배치)";
    /// `{}` is the scratch side.
    pub LAYOUT_8K =
        ko "키 8개를 한 줄로, 스크래치는 {} (F2: 교체, F3: 키 6개 + 트리거)",
        en "키 8개를 한 줄로, 스크래치는 {} (F2: 교체, F3: 키 6개 + 트리거)",
        ja "키 8개를 한 줄로, 스크래치는 {} (F2: 교체, F3: 키 6개 + 트리거)";
    pub LAYOUT_STRAIGHT =
        ko "키를 한 줄로 배치",
        en "키를 한 줄로 배치",
        ja "키를 한 줄로 배치";
    /// `{}` is the scratch side.
    pub LAYOUT_SCRATCH =
        ko "스크래치는 {} (F2: 교체)",
        en "스크래치는 {} (F2: 교체)",
        ja "스크래치는 {} (F2: 교체)";
    pub LAYOUT_PER_MODE =
        ko "키 모드마다 배치를 따로 저장",
        en "키 모드마다 배치를 따로 저장",
        ja "키 모드마다 배치를 따로 저장";
    pub REBIND_REPLACE =
        ko "이 레인에 쓸 키를 누르세요. 레인의 기존 키를 대신하고, 다른 곳에서 쓰던 키는 여기로 옮겨져요.",
        en "이 레인에 쓸 키를 누르세요. 레인의 기존 키를 대신하고, 다른 곳에서 쓰던 키는 여기로 옮겨져요.",
        ja "이 레인에 쓸 키를 누르세요. 레인의 기존 키를 대신하고, 다른 곳에서 쓰던 키는 여기로 옮겨져요.";
    pub REBIND_ADD =
        ko "이 레인에 다른 키를 하나 더 누르세요. 다른 레인이 쓰던 키는 여기로 옮겨져요.",
        en "이 레인에 다른 키를 하나 더 누르세요. 다른 레인이 쓰던 키는 여기로 옮겨져요.",
        ja "이 레인에 다른 키를 하나 더 누르세요. 다른 레인이 쓰던 키는 여기로 옮겨져요.";
    pub REBIND_SELECTED =
        ko "Enter로 키를 지정하고, A로 하나 더 추가하며, Backspace로 레인을 비워요.",
        en "Enter로 키를 지정하고, A로 하나 더 추가하며, Backspace로 레인을 비워요.",
        ja "Enter로 키를 지정하고, A로 하나 더 추가하며, Backspace로 레인을 비워요.";
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
        for (name, texts) in ALL {
            let text = texts[Lang::Ko as usize];
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
