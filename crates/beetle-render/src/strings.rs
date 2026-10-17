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

impl Lang {
    /// Every language, in the order the Settings row cycles them.
    pub const ALL: [Lang; 3] = [Lang::Ko, Lang::En, Lang::Ja];

    /// The value kept in the config file (`language=KO`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Ko => "KO",
            Lang::En => "EN",
            Lang::Ja => "JA",
        }
    }

    /// Reads a config value or an `BEETLE_LANG` value, in any letter case.
    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL
            .into_iter()
            .find(|lang| lang.code().eq_ignore_ascii_case(code.trim()))
    }

    /// The next language in `ALL`, wrapping around.
    pub fn next(self) -> Lang {
        Lang::ALL[(self as usize + 1) % Lang::ALL.len()]
    }

    /// The previous language in `ALL`, wrapping around.
    pub fn prev(self) -> Lang {
        Lang::ALL[(self as usize + Lang::ALL.len() - 1) % Lang::ALL.len()]
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
        en "Beetle - BMS Rhythm Game",
        ja "Beetle - BMS リズムゲーム";
    /// Caption of the error dialog.
    pub APP_NAME =
        ko "Beetle",
        en "Beetle",
        ja "Beetle";
    /// Error dialog when Direct3D 11 cannot start; `{}` is the error.
    pub FATAL_D3D11 =
        ko "Beetle이 Direct3D 11(하드웨어 또는 WARP)을 시작하지 못했어요.\n\n{}",
        en "Beetle could not start Direct3D 11 (hardware or WARP).\n\n{}",
        ja "Beetle は Direct3D 11（ハードウェアまたは WARP）を起動できませんでした。\n\n{}";
    pub CANCEL =
        ko "취소",
        en "Cancel",
        ja "キャンセル";
    pub BACK =
        ko "뒤로",
        en "Back",
        ja "戻る";
    pub LOADING =
        ko "불러오는 중",
        en "Loading",
        ja "読み込み中";

    // ---- Toasts (short notices over the menus) ------------------------
    /// `{}` is the number of songs read.
    pub TOAST_RESCAN_DONE =
        ko "곡 {}개를 읽었어요",
        en "Songs found: {}",
        ja "{}曲を読み込みました";
    /// `{}` is the file name of the saved screenshot.
    pub TOAST_SCREENSHOT_SAVED =
        ko "스크린샷을 저장했어요: {}",
        en "Screenshot saved: {}",
        ja "スクリーンショットを保存しました: {}";
    /// `{}` is the error.
    pub TOAST_SCREENSHOT_FAILED =
        ko "스크린샷을 저장하지 못했어요: {}",
        en "Could not save the screenshot: {}",
        ja "スクリーンショットを保存できませんでした: {}";
    /// Shown when leaving Settings with a value that only applies after a restart.
    pub TOAST_RESTART_NEEDED =
        ko "재시작 후 적용돼요",
        en "Takes effect after restart",
        ja "再起動後に反映されます";
    pub TOAST_UNSUPPORTED_FILE =
        ko "지원하지 않는 파일이에요 (.bmsp, .bms, .bme, .bml, .pms)",
        en "Unsupported file (.bmsp, .bms, .bme, .bml, .pms)",
        ja "対応していないファイルです (.bmsp, .bms, .bme, .bml, .pms)";
    /// `{}` is the file name of the package.
    pub TOAST_UNINSTALLED_ONE =
        ko "설치되지 않은 패키지: {} / bpm install로 설치하세요",
        en "Package not installed: {} / install with bpm install",
        ja "未インストールのパッケージ: {} / bpm install でインストールしてください";
    /// The first `{}` is the count, the second the first file name.
    pub TOAST_UNINSTALLED_MANY =
        ko "설치되지 않은 패키지 {}개 ({} 외) / bpm install로 설치하세요",
        en "{} packages not installed ({} and others) / install with bpm install",
        ja "未インストールのパッケージ {}件（{} ほか） / bpm install でインストールしてください";
    /// `{}` is the file name.
    pub TOAST_OPEN_FAILED =
        ko "곡을 열지 못했어요: {}",
        en "Could not open the song: {}",
        ja "曲を開けませんでした: {}";
    /// `{}` is the chart file name. Shown when a chart cannot be read or parsed; nothing plays.
    pub TOAST_CHART_LOAD_FAILED =
        ko "채보를 불러오지 못했어요: {}",
        en "Could not load the chart: {}",
        ja "譜面を読み込めませんでした: {}";
    /// `{}` is the layout name (key mode and preset, e.g. "7K HomeRow (...)").
    pub TOAST_PRESET =
        ko "키 배치: {} 프리셋",
        en "Key layout: {} preset",
        ja "キー配置: {} プリセット";
    /// `{}` is the layout name (key mode and preset).
    pub TOAST_LAYOUT_RESET =
        ko "키 배치를 {} 프리셋으로 되돌렸어요",
        en "Key layout reset to the {} preset",
        ja "キー配置を {} プリセットに戻しました";

    // ---- Title (the first screen after the library is read) -----------
    /// Under the wordmark; `{}` is the app version.
    pub TITLE_TAGLINE =
        ko "BMS Player  v{}",
        en "BMS Player  v{}",
        ja "BMS Player  v{}";
    /// `{}` is the number of songs in the library.
    pub TITLE_SONGS =
        ko "곡 {}개",
        en "Songs: {}",
        ja "{}曲";
    /// The first menu entry: song select. Also the label after the prompt's keycap.
    pub TITLE_START =
        ko "시작",
        en "Start",
        ja "スタート";
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
        en "Starting up",
        ja "起動しています";
    pub BOOT_TITLE_RESCAN =
        ko "서재를 다시 읽는 중",
        en "Rescanning library",
        ja "ライブラリを再読み込みしています";
    pub BOOT_STATUS_STARTUP =
        ko "곡 목록을 읽는 중",
        en "Reading the song list",
        ja "曲リストを読み込んでいます";
    pub BOOT_STATUS_RESCAN =
        ko "곡 폴더를 검색하는 중",
        en "Scanning song folders",
        ja "曲フォルダを検索しています";

    // ---- Song select --------------------------------------------------
    pub FOLDER =
        ko "폴더",
        en "Folder",
        ja "フォルダ";
    pub SORT =
        ko "정렬",
        en "Sort",
        ja "並べ替え";
    pub OPTIONS =
        ko "옵션",
        en "Options",
        ja "オプション";
    /// Top bar button for the Settings screen (F4).
    pub SETTINGS =
        ko "설정",
        en "Settings",
        ja "設定";
    pub SEARCH_PLACEHOLDER =
        ko "제목, 아티스트 검색",
        en "Search title or artist",
        ja "タイトル、アーティストで検索";
    pub PREVIEW =
        ko "미리듣기",
        en "Preview",
        ja "試聴";
    /// Row tag for a chart with no score yet.
    pub NO_PLAY =
        ko "기록 없음",
        en "No record",
        ja "記録なし";
    pub EMPTY_FOLDER =
        ko "이 폴더에 곡이 없어요.",
        en "No songs in this folder.",
        ja "このフォルダに曲がありません。";
    pub EMPTY_HINT =
        ko "곡 파일(.bms, .bme, .bmsp)을 songs 폴더에 넣고 F5를 눌러 다시 읽으세요.",
        en "Put song files (.bms, .bme, .bmsp) in the songs folder and press F5 to reload.",
        ja "曲ファイル（.bms, .bme, .bmsp）を songs フォルダに入れ、F5 で再読み込みしてください。";
    /// `{}` is the search text.
    pub NO_MATCH =
        ko "\"{}\"에 맞는 곡이 없어요.",
        en "No songs match \"{}\".",
        ja "「{}」に一致する曲がありません。";
    pub SEARCH_HINT =
        ko "/ 키로 검색어를 고치고, Esc로 지워요.",
        en "Press / to edit the search, Esc to clear it.",
        ja "検索語を編集、Esc でクリア";
    pub PERSONAL_BEST =
        ko "최고 기록",
        en "Personal best",
        ja "自己ベスト";
    pub NOT_PLAYED =
        ko "아직 플레이하지 않음",
        en "Not played yet",
        ja "未プレイ";
    pub CLEAR_TO_RECORD =
        ko "이 채보를 클리어하면 기록이 남아요.",
        en "Clear this chart to save a record.",
        ja "この譜面をクリアすると記録が残ります。";
    /// `{}` is the gauge name (GROOVE, HARD, ...).
    pub GAUGE_NAME =
        ko "{} 게이지",
        en "{} gauge",
        ja "{}ゲージ";
    /// `{}` is the play count.
    pub PLAY_COUNT =
        ko "{}회 플레이",
        en "Plays: {}",
        ja "{}回プレイ";
    pub ACCURACY =
        ko "정확도",
        en "Score Rate",
        ja "スコアレート";
    pub MAX_COMBO =
        ko "최대 콤보",
        en "Max Combo",
        ja "最大コンボ";
    pub MIN_BP =
        ko "MIN BP",
        en "MIN BP",
        ja "MIN BP";
    pub PLAY =
        ko "플레이",
        en "Play",
        ja "プレイ";
    pub REPLAY =
        ko "리플레이",
        en "Replay",
        ja "リプレイ";
    pub AUTO_PLAY =
        ko "자동 플레이",
        en "Auto Play",
        ja "オートプレイ";
    /// Shown after the folder's song count: `{}` is the count.
    pub SONGS_TOTAL =
        ko " / {} 곡",
        en " / {} songs",
        ja " / {}曲";
    pub BPM =
        ko "BPM",
        en "BPM",
        ja "BPM";
    pub NOTES =
        ko "노트",
        en "Notes",
        ja "ノーツ";
    pub MODE =
        ko "모드",
        en "Mode",
        ja "モード";

    // Footer key captions (keycaps keep their English key names).
    pub FOOTER_MOVE =
        ko "이동",
        en "Move",
        ja "移動";
    pub FOOTER_PLAY =
        ko "플레이",
        en "Play",
        ja "プレイ";
    pub FOOTER_HELP =
        ko "도움말",
        en "Help",
        ja "ヘルプ";

    // Folder tree (song select). Mode tags (7K) and lamp names (PERFECT) stay in English.
    /// The root of the breadcrumb, above the top-level folders.
    pub FOLDER_ROOT =
        ko "전체",
        en "Top",
        ja "最上位";
    pub FOLDER_ALL =
        ko "전체 곡",
        en "All Songs",
        ja "全曲";
    pub FOLDER_MODE =
        ko "키 모드",
        en "Key Mode",
        ja "キーモード";
    pub FOLDER_LEVEL =
        ko "레벨",
        en "Level",
        ja "レベル";
    pub FOLDER_LAMP =
        ko "클리어 램프",
        en "Clear Lamp",
        ja "クリアランプ";
    pub FOLDER_TABLE =
        ko "난이도표",
        en "Difficulty Table",
        ja "難易度表";
    /// The first folder of 난이도표: every song in any table.
    pub FOLDER_TABLE_ALL =
        ko "전곡",
        en "All Songs",
        ja "全曲";
    /// Detail panel and footer count of a folder: `{}` is the song count.
    pub FOLDER_SONGS =
        ko "{} 곡",
        en "Songs: {}",
        ja "{}曲";
    /// Footer count while the list is folders only: `{}` is the folder count.
    pub FOLDER_COUNT =
        ko "{} 폴더",
        en "Folders: {}",
        ja "{}フォルダ";
    pub FOLDER_OPEN_HINT =
        ko "ENTER로 열고, BKSP로 위 폴더로 나가요.",
        en "Press ENTER to open, BKSP to go up a folder.",
        ja "ENTER で開き、BKSP で上のフォルダに戻ります。";
    pub FOOTER_OPEN =
        ko "열기",
        en "Open",
        ja "開く";

    // Sort names.
    pub SORT_TITLE =
        ko "제목",
        en "Title",
        ja "タイトル";
    pub SORT_LEVEL =
        ko "레벨",
        en "Level",
        ja "レベル";
    pub SORT_CLEAR_LAMP =
        ko "클리어",
        en "Clear Lamp",
        ja "クリアランプ";
    pub SORT_SCORE_RATE =
        ko "정확도",
        en "Score Rate",
        ja "スコアレート";
    pub SORT_BPM =
        ko "BPM",
        en "BPM",
        ja "BPM";

    // Filter row and result count (song select, F10 focuses the row).
    pub FILTER =
        ko "필터",
        en "Filter",
        ja "フィルター";
    pub FILTER_LEVEL =
        ko "레벨",
        en "Level",
        ja "レベル";
    /// The level bounds while they are not set.
    pub FILTER_LEVEL_MIN_ANY =
        ko "최소",
        en "Min",
        ja "最小";
    pub FILTER_LEVEL_MAX_ANY =
        ko "최대",
        en "Max",
        ja "最大";
    pub FILTER_UNPLAYED =
        ko "미플레이만",
        en "Unplayed only",
        ja "未プレイのみ";
    pub FILTER_UNCLEARED =
        ko "미클리어만",
        en "Uncleared only",
        ja "未クリアのみ";
    pub FILTER_RESET =
        ko "초기화",
        en "Reset",
        ja "リセット";
    /// Empty list while a filter is on and there is no search.
    pub FILTER_NO_MATCH =
        ko "필터에 맞는 곡이 없어요.",
        en "No songs match the filter.",
        ja "フィルターに一致する曲がありません。";
    pub FILTER_EMPTY_HINT =
        ko "필터를 바꾸거나 초기화하세요.",
        en "Change or reset the filter.",
        ja "フィルターを変更するかリセットしてください。";
    /// `{}` is the number of songs found.
    pub RESULT_COUNT =
        ko "{}곡 찾음",
        en "Matches: {}",
        ja "{}曲見つかりました";

    // Help overlay (song select, ? key). Keys stay English keycaps.
    pub HELP_TITLE =
        ko "도움말",
        en "Help",
        ja "ヘルプ";
    pub HELP_CLOSE =
        ko "? 또는 ESC로 닫아요",
        en "Press ? or ESC to close",
        ja "? または ESC で閉じます";
    pub HELP_GROUP_MOVE =
        ko "이동",
        en "Move",
        ja "移動";
    pub HELP_GROUP_FOLDER =
        ko "폴더",
        en "Folders",
        ja "フォルダ";
    pub HELP_GROUP_SONG =
        ko "곡",
        en "Songs",
        ja "曲";
    pub HELP_GROUP_OPTIONS =
        ko "옵션과 설정",
        en "Options and Settings",
        ja "オプションと設定";
    pub HELP_GROUP_FILTER =
        ko "필터와 정렬",
        en "Filter and Sort",
        ja "フィルターと並べ替え";
    pub HELP_GROUP_MISC =
        ko "기타",
        en "Other",
        ja "その他";
    pub HELP_CAP_MOUSE =
        ko "마우스",
        en "Mouse",
        ja "マウス";
    pub HELP_CAP_DROP =
        ko "끌어다 놓기",
        en "Drag and drop",
        ja "ドラッグ&ドロップ";
    pub HELP_MOVE_ONE =
        ko "한 줄씩 움직여요",
        en "Move one row at a time",
        ja "1行ずつ移動します";
    pub HELP_MOVE_PAGE =
        ko "10줄씩 움직여요",
        en "Move ten rows at a time",
        ja "10行ずつ移動します";
    pub HELP_MOVE_ENDS =
        ko "처음과 끝으로 가요",
        en "Jump to the first or last row",
        ja "先頭と末尾に移動します";
    pub HELP_MOUSE_WHEEL =
        ko "휠로 목록을 굴려요",
        en "Scroll the list with the wheel",
        ja "ホイールで一覧をスクロールします";
    pub HELP_MOUSE_ROW =
        ko "줄을 누르면 골라요",
        en "Click a row to pick it",
        ja "行をクリックして選びます";
    pub HELP_FOLDER_UP =
        ko "한 단계 위 폴더로 가요",
        en "Go up one folder level",
        ja "一つ上のフォルダに戻ります";
    pub HELP_FOLDER_OPEN =
        ko "폴더는 열고 곡은 재생해요",
        en "Open a folder, or play a song",
        ja "フォルダは開き、曲はプレイします";
    pub HELP_FOLDER_SIDE =
        ko "같은 단계의 이전과 다음 폴더",
        en "Previous and next folder at the same level",
        ja "同じ階層の前後のフォルダ";
    pub HELP_MOUSE_SIDE =
        ko "상단 < > 버튼으로 옆 폴더",
        en "Top < > buttons: adjacent folder",
        ja "上部の < > ボタンで隣のフォルダ";
    pub HELP_MOUSE_CRUMB =
        ko "빵부스러기를 누르면 그 폴더",
        en "Click a breadcrumb to go to that folder",
        ja "パンくずをクリックするとそのフォルダに移動します";
    pub HELP_SONG_PLAY =
        ko "선택한 곡을 플레이해요",
        en "Play the selected song",
        ja "選んだ曲をプレイします";
    pub HELP_SONG_AUTO =
        ko "자동 플레이를 켜고 꺼요",
        en "Turn Auto Play on or off",
        ja "オートプレイのオン・オフを切り替えます";
    pub HELP_SONG_REPLAY =
        ko "저장된 리플레이를 재생해요",
        en "Play a saved replay",
        ja "保存されたリプレイを再生します";
    pub HELP_SONG_CHART =
        ko "묶음 행의 채보를 바꿔요",
        en "Change the chart of a grouped row",
        ja "グループ行の譜面を切り替えます";
    pub HELP_MOUSE_AGAIN =
        ko "고른 줄을 다시 누르면 재생",
        en "Click the selected row again to play",
        ja "選んだ行をもう一度クリックするとプレイします";
    pub HELP_OPTIONS =
        ko "플레이 옵션을 열어요",
        en "Open play options",
        ja "プレイオプションを開きます";
    pub HELP_SETTINGS =
        ko "설정 화면을 열어요",
        en "Open the Settings screen",
        ja "設定画面を開きます";
    pub HELP_KEYS =
        ko "키 설정을 열어요",
        en "Open Key Config",
        ja "キー設定を開きます";
    pub HELP_MOUSE_BUTTONS =
        ko "상단의 옵션, 설정 버튼",
        en "Options and Settings buttons at the top",
        ja "上部のオプション、設定ボタン";
    pub HELP_SEARCH =
        ko "검색창을 열어요",
        en "Open search",
        ja "検索欄を開きます";
    pub HELP_SORT =
        ko "정렬 기준을 바꿔요",
        en "Change the sort order",
        ja "並べ替えの基準を変更します";
    pub HELP_FILTER =
        ko "필터 줄에 초점을 줘요",
        en "Move focus to the filter row",
        ja "フィルター行にフォーカスを移します";
    pub HELP_MOUSE_FILTER =
        ko "정렬, 필터 칩, 검색창 누르기",
        en "Click sort, filter chips, or the search box",
        ja "並べ替え、フィルターチップ、検索欄をクリック";
    pub HELP_BACK_QUIT =
        ko "한 단계 위, 루트에서는 종료",
        en "Go up one level; quit at the top level",
        ja "一つ上の階層へ。最上位では終了します";
    pub HELP_RESCAN =
        ko "곡 목록을 다시 읽어요",
        en "Rescan the song list",
        ja "曲リストを再読み込みします";
    pub HELP_HELP =
        ko "도움말을 열고 닫아요",
        en "Open or close help",
        ja "ヘルプを開閉します";
    pub HELP_DROP =
        ko "파일을 끌어다 놓으면 곡을 열어요",
        en "Drop a file here to open the song",
        ja "ファイルをドロップすると曲を開きます";

    // First run: the library has no songs. The guide lists three ways to add songs.
    pub GUIDE_TITLE =
        ko "아직 곡이 없어요",
        en "No songs yet",
        ja "まだ曲がありません";
    pub GUIDE_LEAD =
        ko "곡을 넣는 방법은 세 가지예요.",
        en "There are three ways to add songs.",
        ja "曲を追加する方法は3つあります。";
    pub GUIDE_MANAGER =
        ko "곡 관리자로 곡 패키지(.bmsp)를 설치해요.",
        en "Install song packages (.bmsp) with the Song Manager.",
        ja "曲管理ツールで曲パッケージ（.bmsp）をインストールします。";
    pub GUIDE_DROP =
        ko "곡 파일(.bmsp, .bms)이나 폴더를 창에 끌어다 놓아요.",
        en "Drag song files (.bmsp, .bms) or folders into this window.",
        ja "曲ファイル（.bmsp, .bms）やフォルダをウィンドウにドラッグ&ドロップします。";
    pub GUIDE_FOLDER =
        ko "곡 파일을 songs 폴더에 넣어요.",
        en "Put song files in the songs folder.",
        ja "曲ファイルを songs フォルダに入れます。";
    pub GUIDE_BTN_MANAGER =
        ko "곡 관리자 열기",
        en "Open Song Manager",
        ja "曲管理ツールを開く";
    pub GUIDE_BTN_FOLDER =
        ko "songs 폴더 열기",
        en "Open songs folder",
        ja "songs フォルダを開く";

    // Drag overlay: a file is dragged over the window (song select).
    pub DROP_TITLE =
        ko "여기에 놓으면 곡을 열어요",
        en "Drop here to open the song",
        ja "ここにドロップして曲を開く";
    pub DROP_NOTE =
        ko ".bmsp 패키지나 .bms 채보를 놓으면 바로 열어요.",
        en "Drop a .bmsp package or .bms chart to open it at once.",
        ja ".bmsp パッケージや .bms 譜面をドロップすると、すぐに開きます。";

    // Opening the song manager or the songs folder failed.
    pub TOAST_MANAGER_OPENED =
        ko "곡을 넣은 뒤 이 창으로 돌아오면 목록이 갱신돼요",
        en "Come back to this window after adding songs to refresh the list",
        ja "曲を追加してからこのウィンドウに戻ると、一覧が更新されます";
    pub TOAST_MANAGER_MISSING =
        ko "곡 관리자(bpm-gui.exe)를 찾지 못했어요",
        en "Could not find the Song Manager (bpm-gui.exe)",
        ja "曲管理ツール（bpm-gui.exe）が見つかりませんでした";
    pub TOAST_FOLDER_FAILED =
        ko "songs 폴더를 열지 못했어요",
        en "Could not open the songs folder",
        ja "songs フォルダを開けませんでした";

    // Play options panel (song select, TAB / O): per-play values only.
    pub MODAL_PLAY_OPTIONS =
        ko "플레이 옵션",
        en "Play Options",
        ja "プレイオプション";
    pub GROUP_PLAY =
        ko "플레이",
        en "Play",
        ja "プレイ";
    pub GROUP_SESSION =
        ko "세션",
        en "Session",
        ja "セッション";
    pub GROUP_DISPLAY =
        ko "화면",
        en "Display",
        ja "画面";
    pub GROUP_AUDIO =
        ko "소리",
        en "Sound",
        ja "サウンド";
    pub GROUP_JUDGE =
        ko "판정",
        en "Judge",
        ja "判定";
    pub GROUP_LAYOUT =
        ko "레이아웃",
        en "Layout",
        ja "レイアウト";
    pub GROUP_INPUT =
        ko "입력",
        en "Input",
        ja "入力";
    pub ROW_GREEN =
        ko "그린 넘버",
        en "Green Number",
        ja "緑数字";
    pub ROW_LANE_COVER =
        ko "레인 커버",
        en "Lane Cover",
        ja "レーンカバー";
    pub ROW_MODIFIER =
        ko "모디파이어",
        en "Modifier",
        ja "配置オプション";
    pub ROW_GAUGE =
        ko "게이지",
        en "Gauge",
        ja "ゲージ";
    pub ROW_LN_MODE =
        ko "LN 모드",
        en "LN Mode",
        ja "LN モード";
    pub ROW_AUTO_PLAY =
        ko "자동 플레이",
        en "Auto Play",
        ja "オートプレイ";
    pub ROW_START_MEASURE =
        ko "시작 마디",
        en "Start Measure",
        ja "開始小節";
    pub ROW_JUDGE_OFFSET =
        ko "판정 오프셋",
        en "Judge Offset",
        ja "判定オフセット";
    pub ROW_MASTER_VOLUME =
        ko "전체 볼륨",
        en "Master Volume",
        ja "マスター音量";
    pub ROW_BGM_VOLUME =
        ko "배경음 볼륨",
        en "BGM Volume",
        ja "BGM 音量";
    pub ROW_KEY_VOLUME =
        ko "키음 볼륨",
        en "Key Volume",
        ja "キー音の音量";
    pub ROW_SFX_VOLUME =
        ko "효과음 볼륨",
        en "SFX Volume",
        ja "効果音の音量";
    pub ROW_VISUAL_OFFSET =
        ko "비주얼 오프셋",
        en "Visual Offset",
        ja "表示オフセット";
    pub ROW_JUDGE_POSITION =
        ko "판정 표시 위치",
        en "Judge Position",
        ja "判定表示位置";
    pub JUDGE_POS_UPPER =
        ko "위",
        en "Upper",
        ja "上";
    pub JUDGE_POS_MIDDLE =
        ko "가운데",
        en "Middle",
        ja "中央";
    pub JUDGE_POS_LOWER =
        ko "아래",
        en "Lower",
        ja "下";
    pub ROW_NOTE_SIZE =
        ko "노트 크기",
        en "Note Size",
        ja "ノーツサイズ";
    pub ROW_REDUCE_FLASHES =
        ko "플래시 줄이기",
        en "Reduce Flashes",
        ja "フラッシュを抑える";
    pub ROW_PLAYFIELD =
        ko "플레이필드",
        en "Playfield",
        ja "プレイフィールド";
    pub ROW_SCRATCH =
        ko "스크래치",
        en "Scratch",
        ja "スクラッチ";
    pub ROW_BGA =
        ko "BGA",
        en "BGA",
        ja "BGA";
    pub ROW_TRACK_BGA =
        ko "트랙 BGA",
        en "Track BGA",
        ja "トラック BGA";
    pub ROW_KEY_HINT =
        ko "플레이 중 키 안내",
        en "Key Hints While Playing",
        ja "プレイ中のキーヒント";
    pub KEY_HINT_FIRST =
        ko "처음 3초만",
        en "First 3 seconds",
        ja "最初の3秒だけ";
    pub KEY_HINT_ALWAYS =
        ko "항상",
        en "Always",
        ja "常に";
    pub KEY_HINT_OFF =
        ko "끄기",
        en "Off",
        ja "オフ";
    pub ROW_DISPLAY_MODE =
        ko "화면 모드",
        en "Display Mode",
        ja "画面モード";
    pub ROW_RESOLUTION =
        ko "해상도",
        en "Resolution",
        ja "解像度";
    pub ROW_GRAPHICS =
        ko "그래픽",
        en "Graphics",
        ja "描画方式";
    pub ROW_TARGET_FPS =
        ko "목표 FPS",
        en "Target FPS",
        ja "目標 FPS";
    pub ROW_KEY_LAYOUT =
        ko "키 배치",
        en "Key Layout",
        ja "キー配置";
    pub VALUE_ON =
        ko "켜짐",
        en "On",
        ja "オン";
    pub VALUE_OFF =
        ko "꺼짐",
        en "Off",
        ja "オフ";
    pub VALUE_UNLIMITED =
        ko "무제한",
        en "Unlimited",
        ja "無制限";
    pub VALUE_CENTER =
        ko "가운데",
        en "Center",
        ja "中央";
    /// A row that does not apply to the selected song's key mode.
    pub VALUE_NOT_APPLICABLE =
        ko "해당 없음",
        en "N/A",
        ja "対象外";
    /// Resolution label when the window size matches no preset.
    pub RESOLUTION_CUSTOM =
        ko "사용자 지정",
        en "Custom",
        ja "カスタム";
    pub DISPLAY_WINDOWED =
        ko "창 모드",
        en "Windowed",
        ja "ウィンドウモード";
    pub DISPLAY_BORDERLESS =
        ko "테두리 없는 창",
        en "Borderless Window",
        ja "ボーダーレスウィンドウ";
    pub DISPLAY_FULLSCREEN =
        ko "전체 화면",
        en "Fullscreen",
        ja "フルスクリーン";
    pub GPU_AUTO =
        ko "자동",
        en "Auto",
        ja "自動";
    /// Backend name, kept in English.
    pub GPU_WARP =
        ko "WARP (CPU)",
        en "WARP (CPU)",
        ja "WARP (CPU)";
    pub TRACK_BGA_OFF =
        ko "꺼짐 (0%)",
        en "Off (0%)",
        ja "オフ (0%)";
    pub TRACK_BGA_LOW =
        ko "낮음 (25%)",
        en "Low (25%)",
        ja "低 (25%)";
    pub TRACK_BGA_MEDIUM =
        ko "보통 (50%)",
        en "Medium (50%)",
        ja "中 (50%)";
    pub TRACK_BGA_HIGH =
        ko "높음 (75%)",
        en "High (75%)",
        ja "高 (75%)";
    /// `{}` is the graphics backend; the note follows it.
    pub VALUE_AFTER_RESTART =
        ko "{} (재시작 후 적용)",
        en "{} (applies after restart)",
        ja "{}（再起動後に適用）";
    /// `{}` is the FPS number.
    pub VALUE_FPS =
        ko "{} FPS",
        en "{} FPS",
        ja "{} FPS";
    /// `{}` is the LN rule AUTO resolves to for the highlighted song.
    pub VALUE_AUTO_RESOLVED =
        ko "자동 ({})",
        en "Auto ({})",
        ja "自動 ({})";
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
        en "Move",
        ja "移動";
    pub HINT_CHANGE =
        ko "변경",
        en "Change",
        ja "変更";
    pub HINT_CLOSE =
        ko "닫기",
        en "Close",
        ja "閉じる";
    pub HINT_START =
        ko "시작",
        en "Start",
        ja "開始";
    pub HINT_SELECT =
        ko "선택",
        en "Select",
        ja "選択";

    // Help sentences: one line under the highlighted row (plain Korean).
    pub HELP_GREEN =
        ko "노트가 화면에 나타나서 판정선에 닿기까지의 시간이에요. 작을수록 빨라요. 레인 커버를 바꿔도 유지돼요.",
        en "Time from a note appearing on screen to reaching the judge line. Lower is faster. It stays the same when the lane cover changes.",
        ja "ノーツが画面に現れてから判定ラインに届くまでの時間です。小さいほど速くなります。レーンカバーを変えても保たれます。";
    pub HELP_LANE_COVER =
        ko "레인 위쪽을 가려 노트가 보이는 구간을 줄여요. 0%는 가리지 않아요. 플레이 중 F10, F11로도 바꿀 수 있어요.",
        en "Hides the top of the lane to shorten the visible note area. 0% hides nothing. You can also change it while playing with F10 and F11.",
        ja "レーン上部を隠して、ノーツが見える範囲を短くします。0% は隠しません。プレイ中に F10、F11 でも変えられます。";
    pub HELP_MODIFIER =
        ko "노트 배치를 바꿔요. 미러는 좌우를 뒤집고, 랜덤 계열은 레인 순서를 섞어요.",
        en "Changes the note arrangement. Mirror flips left and right; random types shuffle the lane order.",
        ja "ノーツの並びを変えます。ミラーは左右を反転し、ランダム系はレーンの順番を入れ替えます。";
    pub HELP_GAUGE =
        ko "점수 게이지의 방식이에요. EASY와 GROOVE는 관대하고, HARD와 HAZARD는 실수에 엄격해요.",
        en "The score gauge style. EASY and GROOVE are forgiving; HARD and HAZARD punish mistakes.",
        ja "スコアゲージの方式です。EASY と GROOVE は寛容で、HARD と HAZARD はミスに厳しくなります。";
    pub HELP_LN_MODE =
        ko "롱노트 판정 방식이에요. AUTO는 곡 데이터에 맞추고, LN과 CN은 어떤 곡이든 그 방식으로 고정해요.",
        en "How long notes are judged. AUTO follows the chart; LN and CN lock every chart to that style.",
        ja "ロングノーツの判定方式です。AUTO は譜面に従い、LN と CN はどの譜面でもその方式に固定します。";
    pub HELP_AUTO_PLAY =
        ko "켜면 키를 누르지 않아도 곡이 끝까지 연주돼요. 자동 플레이 결과는 기록으로 저장되지 않아요.",
        en "Plays the song to the end without key presses. Auto Play results are not saved as records.",
        ja "キー入力なしで最後まで演奏します。オートプレイの結果は記録に保存されません。";
    pub HELP_START_MEASURE =
        ko "이 마디부터 곡을 시작해요. 0이면 처음부터예요. 0이 아니면 연습으로 처리되어 기록되지 않아요.",
        en "Starts the song at this measure. 0 starts from the beginning. Any other value counts as practice and is not saved.",
        ja "この小節から曲を始めます。0 なら最初から始まります。0 以外のときは練習扱いとなり記録されません。";

    // Settings screen (its title is SETTINGS above).
    pub HELP_DISPLAY_MODE =
        ko "창 모드, 테두리 없는 창, 전체 화면 중에서 골라요. 바로 적용돼요.",
        en "Choose windowed, borderless window, or fullscreen. Applies immediately.",
        ja "ウィンドウ、ボーダーレスウィンドウ、フルスクリーンから選びます。すぐに反映されます。";
    pub HELP_RESOLUTION =
        ko "창 크기나 전체 화면 해상도를 골라요. 창 모드는 16:9 크기만 고를 수 있어요.",
        en "Choose the window size or the fullscreen resolution. Windowed mode only offers 16:9 sizes.",
        ja "ウィンドウサイズまたはフルスクリーンの解像度を選びます。ウィンドウでは 16:9 のサイズだけ選べます。";
    pub HELP_GRAPHICS =
        ko "그림을 그리는 장치예요. 자동은 그래픽 카드를 쓰고, WARP는 CPU로 그려요. 바꾸면 재시작 후 적용돼요.",
        en "The drawing device. Auto uses the graphics card; WARP draws on the CPU. Applies after restart when changed.",
        ja "描画に使う装置です。自動はグラフィックカードを使い、WARP は CPU で描画します。変更すると再起動後に反映されます。";
    pub HELP_TARGET_FPS =
        ko "플레이 화면의 최대 프레임이에요. 무제한은 제한하지 않고, 60은 화면 주사율에 맞춰요.",
        en "Maximum frame rate of the play screen. Unlimited does not cap it; 60 matches the display refresh rate.",
        ja "プレイ画面の最大フレームレートです。無制限は制限せず、60 はディスプレイの更新レートに合わせます。";
    pub HELP_MASTER_VOLUME =
        ko "모든 소리의 전체 크기예요. 아래 세 볼륨에 곱해지며, 최대 200%까지 올릴 수 있어요.",
        en "Overall volume for all sound. It multiplies the three volumes below and can go up to 200%.",
        ja "すべての音をまとめた音量です。下の 3 つの音量に掛けられ、200% まで上げられます。";
    pub HELP_BGM_VOLUME =
        ko "곡의 배경음 크기예요. 채보에 들어 있는 자동 재생 소리가 여기에 해당해요. 전체 볼륨과 함께 적용돼요.",
        en "Volume of the song's background audio, including the sounds the chart plays automatically. Applies together with Master Volume.",
        ja "曲の BGM の音量です。譜面に含まれる自動再生の音がこれにあたります。全体音量と合わせて適用されます。";
    pub HELP_KEY_VOLUME =
        ko "플레이 중 내가 누른 키의 소리 크기예요. 판정된 노트의 소리도 여기에 해당해요. 자동 플레이 소리는 배경음이에요.",
        en "Volume of the key sounds you play during a song, including the sounds of notes you hit. Auto Play sounds count as background audio.",
        ja "プレイ中に押したキーの音量です。判定されたノーツの音もここに含まれます。オートプレイの音は BGM 扱いです。";
    pub HELP_SFX_VOLUME =
        ko "메뉴 효과음과 징글의 크기예요. 곡 미리듣기는 배경음 볼륨을 따라요.",
        en "Volume of menu sound effects and jingles. Song preview follows the BGM volume.",
        ja "メニューの効果音とジングルの音量です。曲の試聴は BGM 音量に従います。";
    pub HELP_JUDGE_OFFSET =
        ko "판정 시각을 앞뒤로 옮겨서 판정 결과가 바뀌어요. 범위는 -100에서 +100 ms예요. 화면만 옮기려면 비주얼 오프셋을 써요.",
        en "Shifts judge timing earlier or later, which changes judge results. Range is -100 to +100 ms. To move only the picture, use Visual Offset.",
        ja "判定の時刻を前後にずらし、判定結果を変えます。範囲は -100 から +100 ms です。映像だけをずらすには表示オフセットを使います。";
    pub HELP_VISUAL_OFFSET =
        ko "노트를 그리는 위치만 앞뒤로 옮겨요. 양수면 노트가 더 일찍 보여요. 판정과 소리 타이밍은 바뀌지 않아요.",
        en "Shifts only where notes are drawn. A positive value shows notes sooner. Judge and sound timing do not change.",
        ja "ノーツを描く位置だけを前後にずらします。プラスにするとノーツが早く見えます。判定と音のタイミングは変わりません。";
    pub HELP_JUDGE_POSITION =
        ko "판정과 콤보 글자가 판정선에서 얼마나 떨어져 보일지 정해요. 위가 기본이에요.",
        en "How far the judge and combo text sit from the judge line. Upper is the default.",
        ja "判定とコンボの文字が判定ラインからどれだけ離れて見えるかを決めます。既定は上です。";
    pub HELP_NOTE_SIZE =
        ko "싱글 플레이의 레인 너비를 90%에서 120%까지 바꿔요. 더블 플레이는 그대로예요.",
        en "Changes the lane width in single play from 90% to 120%. Double play does not change.",
        ja "シングルプレイのレーン幅を 90% から 120% の間で変えます。ダブルプレイは変わりません。";
    pub HELP_REDUCE_FLASHES =
        ko "빨간 경고 번쩍임, 판정 불빛, 배너의 빛 줄기와 흔들림을 줄여요. 판정과 소리는 그대로예요.",
        en "Reduces the red warning flash, judge lights, and the banner's light rays and shaking. Judgement and sound stay the same.",
        ja "赤い警告の点滅、判定の光、バナーの光の筋と揺れを抑えます。判定と音は変わりません。";

    pub ROW_REDUCE_MOTION =
        ko "모션 줄이기",
        en "Reduce Motion",
        ja "アニメーションを減らす";

    pub HELP_REDUCE_MOTION =
        ko "화면 전환을 페이드로 바꾸고, 배경의 움직임과 제목 효과를 멈추고, 목록 스크롤을 바로 옮겨요. 기능은 그대로예요.",
        en "Uses a plain fade between screens, stops the moving backgrounds and title effects, and snaps list scrolling. Gameplay is unaffected.",
        ja "画面の切り替えをフェードにし、背景の動きとタイトルの演出を止め、スクロールのアニメーションをなくします。機能は変わりません。";

    pub ROW_HIGH_CONTRAST =
        ko "노트 대비 높이기",
        en "High-Contrast Notes",
        ja "ハイコントラストノーツ";

    pub HELP_HIGH_CONTRAST =
        ko "노트를 더 밝고 선명한 색과 굵은 테두리로 그려요. 판정선은 두껍게, 흔들림 없이 그려지고, 레인 구분선도 더 밝게 그려져요.",
        en "Draws notes in stronger colors with a thick outline, a thicker judge line that does not pulse, and brighter lane dividers.",
        ja "ノーツを濃い色と太い縁取りで描きます。判定ラインは太く、脈動しません。レーンの区切りも明るくなります。";

    pub ROW_UI_SCALE =
        ko "UI 크기",
        en "UI Scale",
        ja "UIサイズ";

    pub HELP_UI_SCALE =
        ko "메뉴와 설정 화면의 글자와 버튼 크기를 80%~150%로 조절해요. 바로 적용돼요.",
        en "Sets the size of menu text and controls, from 80% to 150%. Applies at once.",
        ja "メニューの文字とボタンの大きさを80%～150%で調整します。すぐに反映されます。";
    pub HELP_PLAYFIELD =
        ko "플레이필드를 화면 가운데, 왼쪽, 오른쪽 중 어디에 둘지 정해요.",
        en "Places the playfield in the center, on the left, or on the right of the screen.",
        ja "プレイフィールドを画面の中央、左、右のどこに置くか決めます。";
    pub HELP_SCRATCH =
        ko "스크래치 레인을 왼쪽이나 오른쪽 가장자리에 둬요. 5K, 7K, 8K에서만 적용돼요.",
        en "Puts the scratch lane on the left or right edge. Applies only to 5K, 7K and 8K.",
        ja "スクラッチレーンを左右の端のどちらに置くか決めます。5K、7K、8K でのみ適用されます。";
    pub HELP_BGA =
        ko "곡의 배경 영상과 이미지를 보여 줘요. 끄면 이 파일들을 읽지 않아 메모리와 CPU를 아껴요.",
        en "Shows the song's background movies and images. Off skips reading these files, saving memory and CPU.",
        ja "曲の背景動画と画像を表示します。オフにするとこれらのファイルを読まないため、メモリと CPU を節約できます。";
    pub HELP_KEY_HINT =
        ko "플레이 화면 아래의 키 안내 줄을 정해요. 처음 3초만이면 시작 뒤 3초 동안만 보이고 사라져요.",
        en "Sets the key hint line under the play screen. First 3 seconds shows it for three seconds after the start, then hides it.",
        ja "プレイ画面の下に出すキーヒントの表示を決めます。最初の3秒だけは、開始後3秒間だけ表示して消えます。";
    pub HELP_TRACK_BGA =
        ko "배경 영상과 이미지를 화면 배경에 얼마나 진하게 비출지 정해요. 꺼짐이면 보이지 않아요.",
        en "How strongly the background movie and images show behind the playfield. Off hides them.",
        ja "背景の動画と画像をプレイフィールドの後ろにどれだけ濃く映すかを決めます。オフなら表示しません。";
    pub HELP_KEY_LAYOUT =
        ko "선택한 곡의 키 모드에서 쓸 키 배치를 골라요. ENTER로 키 설정 화면을 열어요.",
        en "Chooses the key layout for the selected song's key mode. Press ENTER to open Key Config.",
        ja "選んだ曲のキーモードで使うキー配置を選びます。ENTER でキー設定画面を開きます。";

    // Judge offset calibration (a Settings sub-screen).
    pub CALIBRATE_TITLE =
        ko "판정 오프셋 측정",
        en "Judge Offset Calibration",
        ja "判定オフセットの測定";
    pub CALIBRATE_INSTRUCTION =
        ko "소리에 맞춰 스페이스를 누르세요",
        en "Press Space in time with the sound",
        ja "音に合わせて Space を押してください";
    pub CALIBRATE_DONE_TITLE =
        ko "측정이 끝났어요",
        en "Calibration complete",
        ja "測定が終わりました";
    pub CALIBRATE_NO_AUDIO =
        ko "오디오 출력 장치를 열지 못해 측정할 수 없어요",
        en "Could not open an audio output device, so calibration is unavailable",
        ja "オーディオ出力デバイスを開けないため、測定できません";
    pub CALIBRATE_COUNT_IN =
        ko "준비",
        en "Get ready",
        ja "準備";
    pub CALIBRATE_MEASURING =
        ko "측정 중",
        en "Measuring",
        ja "測定中";
    pub CALIBRATE_PROGRESS =
        ko "진행",
        en "Progress",
        ja "進捗";
    pub CALIBRATE_MEAN =
        ko "평균",
        en "Mean",
        ja "平均";
    pub CALIBRATE_SPREAD =
        ko "편차",
        en "Spread",
        ja "ばらつき";
    pub CALIBRATE_SUGGEST =
        ko "제안값",
        en "Suggested",
        ja "推奨値";
    pub CALIBRATE_EARLY =
        ko "빠름",
        en "Early",
        ja "早い";
    pub CALIBRATE_LATE =
        ko "늦음",
        en "Late",
        ja "遅い";
    pub CALIBRATE_EXCLUDED =
        ko "제외된 입력은 흐리게 표시해요",
        en "Excluded inputs are shown faded",
        ja "除外された入力は薄く表示されます";
    pub CALIBRATE_HELP =
        ko "준비 박 네 개 뒤부터 입력을 16번 세어요. 한 박에 한 번만 세고, 150 ms보다 멀리 떨어진 입력은 무시해요.",
        en "Counts 16 presses after four count-in beats. Counts one press per beat and ignores presses more than 150 ms away.",
        ja "準備の4拍の後から入力を16回数えます。1拍に1回だけ数え、150 ms より離れた入力は無視します。";
    pub CALIBRATE_HELP_DONE =
        ko "제안값은 평균 차이의 반대 값이에요. 적용하면 판정 오프셋이 이 값이 돼요.",
        en "The suggestion is the mean offset with its sign flipped. Applying it sets the judge offset to this value.",
        ja "推奨値は平均のずれを打ち消す値です。適用すると判定オフセットがこの値になります。";
    pub CALIBRATE_HELP_NO_AUDIO =
        ko "설정은 바뀌지 않았어요. ESC로 설정 화면에 돌아가세요.",
        en "Settings were not changed. Press ESC to return to Settings.",
        ja "設定は変わっていません。ESC で設定画面に戻ってください。";
    pub CALIBRATE_APPLY =
        ko "적용",
        en "Apply",
        ja "適用";
    pub CALIBRATE_RETRY =
        ko "다시",
        en "Retry",
        ja "やり直し";
    pub CALIBRATE_CANCEL =
        ko "취소",
        en "Cancel",
        ja "キャンセル";
    pub CALIBRATE_HINT_TAP =
        ko "누르기",
        en "Tap",
        ja "入力";
    pub TOAST_CALIBRATE_APPLIED =
        ko "판정 오프셋을 {} ms로 맞췄어요",
        en "Judge offset set to {} ms",
        ja "判定オフセットを {} ms に設定しました";

    // Quit confirmation.
    pub QUIT_TITLE =
        ko "BEETLE을 종료할까요?",
        en "Quit BEETLE?",
        ja "BEETLE を終了しますか？";
    pub QUIT_NOTE =
        ko "기록과 설정은 이미 저장되어 있어요.",
        en "Your records and settings are already saved.",
        ja "記録と設定は保存済みです。";
    pub QUIT =
        ko "종료",
        en "Quit",
        ja "終了";

    // ---- Loading ------------------------------------------------------
    pub LOADING_STATUS =
        ko "키음을 디코딩하고 오디오를 준비하는 중",
        en "Decoding key sounds and preparing audio",
        ja "キー音をデコードし、オーディオを準備しています";
    pub PRACTICE =
        ko "연습",
        en "Practice",
        ja "練習";

    // ---- Gameplay -----------------------------------------------------
    pub COMBO =
        ko "콤보",
        en "Combo",
        ja "コンボ";
    pub NO_BGA =
        ko "BGA 없음",
        en "No BGA",
        ja "BGA なし";
    pub PAUSED =
        ko "일시정지",
        en "Paused",
        ja "一時停止";
    pub PAUSE_RESUME =
        ko "계속하기",
        en "Resume",
        ja "再開";
    pub PAUSE_RESTART =
        ko "처음부터",
        en "Restart",
        ja "最初から";
    pub PAUSE_QUIT =
        ko "곡 선택으로",
        en "Back to Song Select",
        ja "曲選択に戻る";
    pub PAUSE_HINT =
        ko "위아래 이동   ENTER 확인   R 처음부터",
        en "Up/Down move   ENTER confirm   R restart",
        ja "上下 移動   ENTER 決定   R 最初から";
    /// Key hint line during a replay or auto play.
    pub HUD_BACK_TO_SELECT =
        ko "ESC  곡 선택으로",
        en "ESC  Back to Song Select",
        ja "ESC  曲選択に戻る";
    /// Key hint line while playing; `{}` is the key layout.
    pub HUD_KEYS =
        ko "키  {}    1/2 그린 넘버    F10/F11 커버    ESC 일시정지",
        en "Keys  {}    1/2 Green Number    F10/F11 Cover    ESC Pause",
        ja "キー  {}    1/2 緑数字    F10/F11 カバー    ESC 一時停止";
    /// Same, for layouts without cover keys (double play).
    pub HUD_KEYS_NO_COVER =
        ko "키  {}    1/2 그린 넘버    ESC 일시정지",
        en "Keys  {}    1/2 Green Number    ESC Pause",
        ja "キー  {}    1/2 緑数字    ESC 一時停止";
    pub KEYS_CUSTOM =
        ko "사용자 배치",
        en "Custom layout",
        ja "カスタム配置";
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
        en "F3/F4 Green Number / F10/F11 Lane Cover",
        ja "F3/F4 緑数字 / F10/F11 レーンカバー";
    /// Readout over the lane after a green number change (`{}` = ms).
    pub READOUT_GREEN =
        ko "{} ms",
        en "{} ms",
        ja "{} ms";
    /// Readout over the lane after a lane cover change (`{}` = percent).
    pub READOUT_COVER =
        ko "커버 {}%",
        en "Cover {}%",
        ja "カバー {}%";
    pub TIMELINE_TITLE =
        ko "판정 타임라인",
        en "Judge Timeline",
        ja "判定タイムライン";
    pub TIMELINE_RANGE =
        ko "최근 8초",
        en "Last 8 seconds",
        ja "直近8秒";
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
        en "Result",
        ja "リザルト";
    pub RESULT_SONG_SELECT =
        ko "곡 선택",
        en "Song Select",
        ja "曲選択";
    pub RESULT_RETRY =
        ko "다시 하기",
        en "Retry",
        ja "もう一度";
    pub RESULT_RETRY_OPTIONS =
        ko "옵션 바꿔 다시 하기",
        en "Retry with Options",
        ja "オプションを変えてもう一度";
    pub RESULT_SCREENSHOT =
        ko "스크린샷",
        en "Screenshot",
        ja "スクリーンショット";
    pub SCORE_NOT_SAVED =
        ko "기록이 저장되지 않음",
        en "Score not saved",
        ja "記録は保存されません";
    /// The button of the play options panel opened from the result: play
    /// again with the options as changed (ENTER).
    pub RESULT_OPTIONS_START =
        ko "새 옵션으로 다시 하기",
        en "Retry with New Options",
        ja "新しいオプションでもう一度";
    /// Trend graph: the clear line (`{}` = gauge percent) and the fail line of
    /// the HARD / HAZARD gauges, which fail at 0.
    pub GRAPH_CLEAR_LINE =
        ko "클리어 {}%",
        en "Clear {}%",
        ja "クリア {}%";
    pub GRAPH_FAIL_LINE =
        ko "실패 0%",
        en "Fail 0%",
        ja "失敗 0%";
    /// Marks the point where the stage failed on the trend graph.
    pub GRAPH_FAILED =
        ko "FAILED",
        en "FAILED",
        ja "FAILED";
    /// Tag next to the clear lamp when the lamp got better.
    pub NEW_LAMP =
        ko "램프 갱신",
        en "Lamp Update",
        ja "ランプ更新";
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
        en "New Record",
        ja "新記録";
    /// `{}` is the gauge name; `{}` is the rule note (e.g. "LN").
    pub GAUGE_NAME_RULE =
        ko "{} 게이지 / {}",
        en "{} gauge / {}",
        ja "{}ゲージ / {}";
    /// `{}` is the best EX score.
    pub BEST_EX =
        ko "최고 {}",
        en "Best {}",
        ja "ベスト {}";
    pub FIRST_PLAY =
        ko "첫 플레이",
        en "First play",
        ja "初プレイ";
    pub JUDGE =
        ko "판정",
        en "Judge",
        ja "判定";
    pub TIMING =
        ko "타이밍",
        en "Timing",
        ja "タイミング";
    pub OFFSET =
        ko "오프셋",
        en "Offset",
        ja "オフセット";
    pub MISS_COUNT =
        ko "MISS 수",
        en "MISS count",
        ja "MISS 数";

    // ---- Key config ---------------------------------------------------
    pub KEY_CONFIG =
        ko "키 설정",
        en "Key Config",
        ja "キー設定";
    pub KEY_LANE =
        ko "레인",
        en "Lane",
        ja "レーン";
    pub SET_KEY =
        ko "키 지정",
        en "Set Key",
        ja "キーを設定";
    pub ADD_KEY =
        ko "키 추가",
        en "Add Key",
        ja "キーを追加";
    pub CLEAR_LANE =
        ko "연결 해제",
        en "Unbind",
        ja "割り当て解除";
    pub PRESET =
        ko "프리셋",
        en "Preset",
        ja "プリセット";
    pub SCRATCH =
        ko "스크래치",
        en "Scratch",
        ja "スクラッチ";
    pub FORM_8K =
        ko "8K 형태",
        en "8K Layout",
        ja "8K 配置";
    pub RESET =
        ko "초기화",
        en "Reset",
        ja "リセット";
    pub ANY_KEY =
        ko "아무 키",
        en "Any Key",
        ja "任意のキー";
    pub BIND =
        ko "지정",
        en "Bind",
        ja "割り当て";
    pub SELECTED_LANE =
        ko "선택한 레인",
        en "Selected Lane",
        ja "選択中のレーン";
    pub KEYS_CAPTION =
        ko "키",
        en "Keys",
        ja "キー";
    /// A controller button in a lane's list ("{}" is its number).
    pub PAD_BUTTON =
        ko "패드 {}",
        en "PAD {}",
        ja "パッド {}";
    pub SIDE_LEFT =
        ko "왼쪽",
        en "Left",
        ja "左";
    pub SIDE_RIGHT =
        ko "오른쪽",
        en "Right",
        ja "右";
    pub LAYOUT_TRIGGERS =
        ko "왼쪽, 오른쪽 트리거 사이에 키 6개 (F3: 일자 배치)",
        en "Six keys between the left and right triggers (F3: straight layout)",
        ja "左右のトリガーの間に6つのキー（F3: 一列配置）";
    /// `{}` is the scratch side.
    pub LAYOUT_8K =
        ko "키 8개를 한 줄로, 스크래치는 {} (F2: 교체, F3: 키 6개 + 트리거)",
        en "Eight keys in a row, scratch: {} (F2: swap, F3: six keys and triggers)",
        ja "8つのキーを一列に並べ、スクラッチは{}（F2: 入れ替え、F3: キー6つ + トリガー）";
    pub LAYOUT_STRAIGHT =
        ko "키를 한 줄로 배치",
        en "Keys in a single row",
        ja "キーを一列に配置";
    /// `{}` is the scratch side.
    pub LAYOUT_SCRATCH =
        ko "스크래치는 {} (F2: 교체)",
        en "Scratch: {} (F2: swap)",
        ja "スクラッチは{}（F2: 入れ替え）";
    pub LAYOUT_PER_MODE =
        ko "키 모드마다 배치를 따로 저장",
        en "Each key mode saves its own layout",
        ja "配置はキーモードごとに別々に保存されます";
    pub REBIND_REPLACE =
        ko "이 레인에 쓸 키나 패드 버튼을 누르세요. 레인의 기존 키를 대신하고, 다른 곳에서 쓰던 키는 여기로 옮겨져요.",
        en "Press a key or pad button for this lane. It replaces the lane's current key, and a key used elsewhere moves here.",
        ja "このレーンに使うキーかパッドのボタンを押してください。レーンの既存のキーと置き換わり、他で使っていたキーはここに移ります。";
    pub REBIND_ADD =
        ko "이 레인에 추가할 키나 패드 버튼을 누르세요. 다른 레인이 쓰던 키는 여기로 옮겨져요.",
        en "Press another key or pad button for this lane. A key used by another lane moves here.",
        ja "追加するキーかパッドのボタンを押してください。他のレーンが使っていたキーはここに移ります。";
    pub REBIND_SELECTED =
        ko "Enter로 키를 지정하고, A로 하나 더 추가하며, Backspace로 레인을 비워요.",
        en "Enter binds a key, A adds one more, Backspace clears the lane.",
        ja "Enter でキーを割り当て、A で追加、Backspace でレーンを消去します。";

    // ---- Language setting (Settings screen, first row) ----
    /// Section of the Settings screen for the language.
    pub GROUP_SYSTEM =
        ko "시스템",
        en "System",
        ja "システム";
    /// Settings row: the UI language.
    pub ROW_LANGUAGE =
        ko "언어",
        en "Language",
        ja "言語";
    /// One sentence under the Language row.
    pub HELP_LANGUAGE =
        ko "메뉴와 글자의 언어를 바꿔요. 바로 적용돼요.",
        en "Changes the language of the menus and text. Applies immediately.",
        ja "メニューと表示の言語を変更します。すぐに反映されます。";
    /// The Korean language's name; shown in Korean whatever the current language is.
    pub LANG_NAME_KO =
        ko "한국어",
        en "한국어",
        ja "한국어";
    /// The English language's name; shown in English whatever the current language is.
    pub LANG_NAME_EN =
        ko "English",
        en "English",
        ja "English";
    /// The Japanese language's name; shown in Japanese whatever the current language is.
    pub LANG_NAME_JA =
        ko "日本語",
        en "日本語",
        ja "日本語";

    // ---- Found after the first capture review ----
    /// Top bar button for Auto Play while it is off.
    pub BUTTON_AUTO =
        ko "자동",
        en "Auto",
        ja "自動";
    /// Error toast when the play cannot open an audio device.
    pub TOAST_NO_AUDIO_DEVICE =
        ko "오디오 장치를 찾지 못했어요",
        en "No audio device found",
        ja "オーディオデバイスが見つかりませんでした";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{JP_BYTES, KR_BYTES};
    use fontdue::{Font, FontSettings};

    /// Every Korean character must be in the embedded KR font, or it would be
    /// drawn by the GDI fallback and look different.
    #[test]
    fn every_korean_character_is_in_the_embedded_kr_font() {
        let font = Font::from_bytes(KR_BYTES, FontSettings::default()).expect("KR font");
        let jp = Font::from_bytes(JP_BYTES, FontSettings::default()).expect("JP font");
        let mut missing = Vec::new();
        for (name, texts) in ALL {
            for c in texts[Lang::Ko as usize].chars() {
                if c.is_control() || c == ' ' {
                    continue;
                }
                // A language's own name may be Japanese (see the English test).
                let in_jp = name.starts_with("LANG_NAME_") && jp.lookup_glyph_index(c) != 0;
                if font.lookup_glyph_index(c) == 0 && !in_jp {
                    missing.push(format!("{name}: {c:?} (U+{:04X})", c as u32));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "glyphs missing from the KR font:
{}",
            missing.join(
                "
"
            )
        );
    }

    /// English is Basic Latin and punctuation, which the KR font has. The only
    /// exception is a language's own name (the Settings row shows 日本語 in
    /// every language), which the JP font covers.
    #[test]
    fn english_is_ascii_except_the_language_names() {
        let kr = Font::from_bytes(KR_BYTES, FontSettings::default()).expect("KR font");
        let jp = Font::from_bytes(JP_BYTES, FontSettings::default()).expect("JP font");
        for (name, texts) in ALL {
            let en = texts[Lang::En as usize];
            if en.is_ascii() {
                continue;
            }
            assert!(
                name.starts_with("LANG_NAME_"),
                "{name} has non-ASCII English: {en}"
            );
            for c in en.chars().filter(|c| !c.is_ascii()) {
                assert!(
                    kr.lookup_glyph_index(c) != 0 || jp.lookup_glyph_index(c) != 0,
                    "{name}: {c:?} is in neither embedded font"
                );
            }
        }
    }

    /// Every Japanese character must be in the embedded JP font, or in the KR
    /// font (Latin, punctuation the KR font has). The text engine tries JP
    /// first and then KR for the same characters.
    #[test]
    fn every_japanese_character_is_in_an_embedded_font() {
        let kr = Font::from_bytes(KR_BYTES, FontSettings::default()).expect("KR font");
        let jp = Font::from_bytes(JP_BYTES, FontSettings::default()).expect("JP font");
        let mut missing = Vec::new();
        for (name, texts) in ALL {
            for c in texts[Lang::Ja as usize].chars() {
                if c.is_control() || c == ' ' {
                    continue;
                }
                if jp.lookup_glyph_index(c) == 0 && kr.lookup_glyph_index(c) == 0 {
                    missing.push(format!("{name}: {c:?} (U+{:04X})", c as u32));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "glyphs missing from the JP and KR fonts:
{}",
            missing.join(
                "
"
            )
        );
    }

    #[test]
    fn language_codes_and_cycling() {
        assert_eq!(Lang::from_code("ja"), Some(Lang::Ja));
        assert_eq!(Lang::from_code(" EN "), Some(Lang::En));
        assert_eq!(Lang::from_code("fr"), None);
        for lang in Lang::ALL {
            assert_eq!(Lang::from_code(lang.code()), Some(lang));
        }
        assert_eq!(Lang::Ko.next(), Lang::En);
        assert_eq!(Lang::Ja.next(), Lang::Ko);
        assert_eq!(Lang::Ko.prev(), Lang::Ja);
    }

    #[test]
    fn every_template_has_the_same_placeholders_in_every_language() {
        for (name, texts) in ALL {
            let counts: Vec<usize> = texts.iter().map(|t| t.matches("{}").count()).collect();
            assert!(
                counts.iter().all(|&n| n == counts[0]),
                "{name}: placeholder counts differ {counts:?}"
            );
        }
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
