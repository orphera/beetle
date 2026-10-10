//! The dialogs: what each one asks, its text fields, and how it looks
//! (`Dialog::view`). What confirming does lives in `main.rs`.

use crate::file_dialog::PickKind;
use crate::ui::{ButtonView, ChoiceView, DialogView, FieldView, ListRowView};
use crate::widgets::{Btn, UiAction};
use bms_package_manager::BgaPackMode;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogKind {
    Help,
    /// Add songs: pick a folder or a file, or type a path.
    Add,
    /// Folders the game reads in place (`library.dat`).
    Library,
    /// Packing and update files, for people who make packages.
    Advanced,
    Pack {
        turbo: bool,
        bga: BgaPackMode,
    },
    ApplyDelta,
    CreateDelta,
    /// Remove a song: every version (`state` None) or one version.
    ConfirmUninstall {
        id: String,
        name: String,
        state: Option<String>,
    },
    ConfirmRemoveBga {
        id: String,
        name: String,
    },
    /// Download the selected table entry's chart into `folder`.
    TableFetchDiff {
        title: String,
        folder: PathBuf,
    },
    /// Download the zip links that an IR chart page lists for the selected entry
    /// into `folder`. `others` counts the links that are not zip files.
    TableFetchIr {
        title: String,
        folder: PathBuf,
        links: Vec<(&'static str, String)>,
        others: usize,
    },
    /// Add the selected table entry from a downloaded song archive.
    TableGetBody {
        title: String,
    },
    AddTable,
}

pub struct Dialog {
    pub kind: DialogKind,
    pub fields: Vec<String>,
    pub focus: usize,
}

const ARCHIVES: PickKind = PickKind::File {
    filter_name: "압축 파일 (zip, rar, 7z)",
    filter_spec: "*.zip;*.rar;*.7z",
};
const SONG_FILES: PickKind = PickKind::File {
    filter_name: "곡 파일 (bmsp, zip, rar, 7z)",
    filter_spec: "*.bmsp;*.zip;*.rar;*.7z",
};
const BMDP: PickKind = PickKind::File {
    filter_name: "업데이트 파일 (.bmdp)",
    filter_spec: "*.bmdp",
};

fn button(label: &str, style: Btn, action: UiAction) -> ButtonView {
    ButtonView {
        label: label.to_string(),
        style,
        action,
    }
}

fn cancel_ok(ok: &str) -> Vec<ButtonView> {
    vec![
        button("취소", Btn::Ghost, UiAction::DialogCancel),
        button(ok, Btn::Primary, UiAction::DialogConfirm),
    ]
}

impl Dialog {
    pub fn new(kind: DialogKind) -> Self {
        let count = match kind {
            DialogKind::Add
            | DialogKind::Library
            | DialogKind::Pack { .. }
            | DialogKind::ApplyDelta
            | DialogKind::TableGetBody { .. }
            | DialogKind::AddTable => 1,
            DialogKind::CreateDelta => 2,
            _ => 0,
        };
        Self {
            kind,
            fields: vec![String::new(); count],
            focus: 0,
        }
    }

    pub fn has_fields(&self) -> bool {
        !self.fields.is_empty()
    }

    pub fn field(&self, i: usize) -> String {
        clean_path(self.fields.get(i).map(String::as_str).unwrap_or(""))
    }

    /// The picker the "찾아보기" button of field `i` opens, with its title.
    pub fn browse(&self, i: usize) -> Option<(PickKind, &'static str)> {
        match (&self.kind, i) {
            (DialogKind::Library, _) => Some((PickKind::Folder, "게임에 연결할 BMS 폴더 고르기")),
            (DialogKind::Pack { .. }, _) => {
                Some((PickKind::Folder, "패키지로 만들 곡 폴더 고르기"))
            }
            (DialogKind::ApplyDelta, _) => Some((BMDP, "적용할 업데이트 파일 고르기")),
            (DialogKind::CreateDelta, 0) => Some((PickKind::Folder, "원래 곡 폴더 고르기")),
            (DialogKind::CreateDelta, _) => Some((PickKind::Folder, "바뀐 곡 폴더 고르기")),
            (DialogKind::TableGetBody { .. }, _) => Some((ARCHIVES, "받은 곡 파일 고르기")),
            _ => None,
        }
    }

    /// The picker each big choice of the dialog opens.
    pub fn card_pick(&self, i: usize) -> Option<(PickKind, &'static str)> {
        match (&self.kind, i) {
            (DialogKind::Add, 0) => Some((PickKind::Folder, "추가할 곡 폴더 고르기")),
            (DialogKind::Add, _) => Some((SONG_FILES, "추가할 곡 파일 고르기")),
            _ => None,
        }
    }

    pub fn view(&self, library: &[String]) -> DialogView {
        let field = |i: usize, label: &str, placeholder: &str| FieldView {
            label: label.to_string(),
            value: self.fields[i].clone(),
            placeholder: placeholder.to_string(),
            focused: self.focus == i,
            browse: self.browse(i).is_some(),
        };
        let mut v = DialogView::default();
        match &self.kind {
            DialogKind::Help => {
                v.title = "Beetle 곡 관리자 사용법".into();
                v.wide = true;
                v.body = vec![
                    "Beetle 게임에서 플레이할 곡을 넣고 정리하는 프로그램이에요.".into(),
                    "BMS란? 리듬게임용 곡 형식이에요. 곡 하나는 음악, 효과음과 채보 파일(.bms 등)이 든 폴더 하나예요.".into(),
                    "[내 곡] 갖고 있는 곡을 보고 지워요. 곡 폴더나 압축 파일을 창에 끌어다 놓으면 추가돼요.".into(),
                    "[바로 설치] Beetle용으로 준비된 곡을 한 번에 받아 설치해요.".into(),
                    "[난이도표] 커뮤니티 추천 곡 목록에서 아직 없는 곡을 찾아 받아요. 처음이라면 여기서 시작하세요.".into(),
                ];
                v.footnote = "단축키: Tab 탭 이동 · 위/아래 화살표 선택 · / 검색 · Enter 확인 · Esc 닫기 · F5 새로고침 · Ctrl+V 붙여넣기".into();
                v.buttons = vec![button("알겠어요", Btn::Primary, UiAction::DialogConfirm)];
            }
            DialogKind::Add => {
                v.title = "곡 추가".into();
                v.body = vec![
                    "추가할 곡을 골라 주세요. 여러 곡이 든 상위 폴더를 골라도 한 번에 추가돼요."
                        .into(),
                ];
                v.cards = vec![
                    (
                        "폴더에서 추가".into(),
                        "압축을 푼 BMS 곡 폴더 (또는 그런 폴더들이 든 폴더)".into(),
                    ),
                    (
                        "파일에서 추가".into(),
                        "곡이 든 압축 파일(.zip/.rar/.7z) 또는 .bmsp 패키지".into(),
                    ),
                ];
                v.fields = vec![field(
                    0,
                    "또는 경로를 직접 붙여넣기",
                    "예: D:\\BMS\\곡 폴더",
                )];
                v.footnote = "창에 끌어다 놓아도 돼요. 추가한 곡은 원본을 복사해 따로 보관하므로 원본은 지워도 괜찮아요.".into();
                v.buttons = cancel_ok("추가");
            }
            DialogKind::Library => {
                v.title = "기존 BMS 폴더 연결".into();
                v.wide = true;
                v.body = vec![
                    "이미 BMS 곡을 모아 둔 폴더가 있다면, 복사하지 않고 그대로 게임에 연결할 수 있어요.".into(),
                ];
                v.list = library
                    .iter()
                    .map(|p| ListRowView {
                        text: p.clone(),
                        ok: Path::new(p).is_dir(),
                    })
                    .collect();
                v.list_empty = "아직 연결한 폴더가 없어요".into();
                v.fields = vec![field(
                    0,
                    "연결할 폴더",
                    "폴더 경로 (찾아보기로 고르면 바로 연결돼요)",
                )];
                v.footnote = "바뀐 내용은 게임을 다시 시작하면 반영돼요.".into();
                v.buttons = vec![
                    button("닫기", Btn::Ghost, UiAction::DialogCancel),
                    button("연결", Btn::Primary, UiAction::DialogConfirm),
                ];
            }
            DialogKind::Advanced => {
                v.title = "고급 도구".into();
                v.body = vec!["곡을 배포하거나 고치는 사람을 위한 기능이에요. 곡을 플레이만 한다면 필요 없어요.".into()];
                v.cards = vec![
                    (
                        "곡 폴더를 패키지(.bmsp)로 만들기".into(),
                        "곡 폴더를 다른 사람에게 주기 쉬운 파일 하나로 묶어요".into(),
                    ),
                    (
                        "업데이트 파일(.bmdp) 적용".into(),
                        "설치된 곡에 받은 업데이트 파일을 적용해요".into(),
                    ),
                    (
                        "업데이트 파일(.bmdp) 만들기".into(),
                        "원래 곡 폴더와 바뀐 곡 폴더의 차이만 담은 파일을 만들어요".into(),
                    ),
                ];
                v.buttons = vec![button("닫기", Btn::Ghost, UiAction::DialogCancel)];
            }
            DialogKind::Pack { turbo, bga } => {
                v.title = "패키지(.bmsp) 만들기".into();
                v.wide = true;
                v.fields = vec![field(0, "곡 폴더", "패키지로 만들 곡 폴더")];
                v.choices = vec![
                    ChoiceView {
                        label: "형식".into(),
                        options: vec!["일반", "빠른 로딩 (Turbo)"],
                        selected: *turbo as usize,
                        note: if *turbo {
                            "소리를 미리 풀어서 담아 게임 로딩이 빨라져요. 대신 파일이 커져요."
                                .into()
                        } else {
                            "원본 소리 파일을 그대로 담아요. 잘 모르겠다면 이걸 고르세요.".into()
                        },
                    },
                    ChoiceView {
                        label: "배경 영상".into(),
                        options: vec!["함께 담기", "따로 분리", "빼기"],
                        selected: match bga {
                            BgaPackMode::Embed => 0,
                            BgaPackMode::Split => 1,
                            BgaPackMode::NoVideo => 2,
                        },
                        note: match bga {
                            BgaPackMode::Embed => "영상까지 파일 하나에 담아요.".into(),
                            BgaPackMode::Split => {
                                "영상을 .bga.bmsp 파일로 따로 만들어, 원하는 사람만 받게 해요."
                                    .into()
                            }
                            BgaPackMode::NoVideo => "영상을 빼서 파일을 가장 작게 만들어요.".into(),
                        },
                    },
                ];
                v.footnote = "만든 파일은 고른 폴더 옆에 저장돼요. 여러 곡이 든 폴더를 고르면 곡마다 하나씩 그 폴더 안에 만들어요.".into();
                v.buttons = cancel_ok("만들기");
            }
            DialogKind::ApplyDelta => {
                v.title = "업데이트 파일 적용".into();
                v.body = vec![
                    "받은 업데이트 파일(.bmdp)을 골라 주세요. 해당 곡의 새 버전이 추가돼요.".into(),
                ];
                v.fields = vec![field(0, "", "업데이트 파일(.bmdp) 경로")];
                v.buttons = cancel_ok("적용");
            }
            DialogKind::CreateDelta => {
                v.title = "업데이트 파일 만들기".into();
                v.wide = true;
                v.body = vec!["두 폴더의 차이만 담은 작은 업데이트 파일(.bmdp)을 만들어요.".into()];
                v.fields = vec![
                    field(0, "원래 곡 폴더", "바뀌기 전 폴더"),
                    field(1, "바뀐 곡 폴더", "바뀐 뒤 폴더"),
                ];
                v.footnote = "만든 파일은 바뀐 곡 폴더 옆에 저장돼요.".into();
                v.buttons = cancel_ok("만들기");
            }
            DialogKind::ConfirmUninstall { name, state, .. } => {
                if state.is_some() {
                    v.title = "이 버전을 삭제할까요?".into();
                    v.body = vec![format!(
                        "'{name}'의 고른 버전을 삭제해요. 다른 버전은 그대로 남아요."
                    )];
                } else {
                    v.title = "곡을 삭제할까요?".into();
                    v.body = vec![
                        format!("'{name}'을(를) 이 컴퓨터에서 삭제해요."),
                        "삭제한 곡은 되돌릴 수 없어요. 다시 쓰려면 다시 추가해야 해요.".into(),
                    ];
                }
                v.buttons = vec![
                    button("취소", Btn::Ghost, UiAction::DialogCancel),
                    button("삭제", Btn::Danger, UiAction::DialogConfirm),
                ];
            }
            DialogKind::ConfirmRemoveBga { name, .. } => {
                v.title = "배경 영상을 지울까요?".into();
                v.body = vec![
                    format!("'{name}'의 배경 영상 파일을 지워 저장 공간을 확보해요."),
                    "곡은 그대로 플레이할 수 있고, 배경에 영상만 나오지 않아요.".into(),
                ];
                v.buttons = vec![
                    button("취소", Btn::Ghost, UiAction::DialogCancel),
                    button("지우기", Btn::Danger, UiAction::DialogConfirm),
                ];
            }
            DialogKind::TableFetchDiff { title, folder } => {
                v.title = "채보를 받을까요?".into();
                v.body = vec![
                    format!("'{title}'의 채보를 내려받아요."),
                    format!("저장 위치: {}", folder.display()),
                ];
                v.footnote = "곡 파일(본체)이 아직 없다면 소리가 나지 않을 수 있어요. 그럴 땐 곡 파일을 먼저 추가해 주세요.".into();
                v.buttons = cancel_ok("받기");
            }
            DialogKind::TableFetchIr {
                title,
                folder,
                links,
                others,
            } => {
                v.title = "IR에서 찾은 채보를 받을까요?".into();
                v.wide = true;
                let mut body = vec![format!(
                    "'{title}'의 압축 파일 {}개를 내려받아요.",
                    links.len()
                )];
                for (label, url) in links {
                    let what = if *label == "body" {
                        "곡 파일(본체)"
                    } else {
                        "채보(차분)"
                    };
                    body.push(format!("{what}: {url}"));
                }
                if *others > 0 {
                    body.push(format!(
                        "zip이 아니라서 받지 않는 링크 {others}개는 브라우저로 열 수 있어요."
                    ));
                }
                body.push(format!("저장 위치: {}", folder.display()));
                v.body = body;
                v.footnote = "받은 파일에 이 곡의 채보가 있을 때만 저장해요. 없으면 아무것도 저장하지 않아요.".into();
                v.buttons = cancel_ok("받기");
            }
            DialogKind::TableGetBody { title } => {
                v.title = "받은 곡 파일로 추가".into();
                v.wide = true;
                v.body = vec![
                    format!("'{title}'의 곡 파일(본체)을 골라 주세요."),
                    "'곡 파일 받으러 가기'로 열린 페이지에서 받은 압축 파일(.zip/.rar/.7z)이에요."
                        .into(),
                ];
                v.fields = vec![field(0, "", "받은 압축 파일 경로")];
                v.footnote = "채보를 바로 받을 수 있는 곡이면 채보도 함께 받아서 추가해요.".into();
                v.buttons = cancel_ok("추가");
            }
            DialogKind::AddTable => {
                v.title = "난이도표 추가".into();
                v.wide = true;
                v.body = vec!["난이도표 웹페이지의 주소를 붙여넣어 주세요.".into()];
                v.fields = vec![field(0, "", "https://...")];
                v.footnote = "난이도표 페이지 주소를 그대로 넣으면 돼요. 표를 받아 오는 데 인터넷 연결이 필요해요.".into();
                v.buttons = cancel_ok("추가");
            }
        }
        v
    }
}

/// A path as typed or pasted: trimmed, without surrounding quotes.
pub fn clean_path(text: &str) -> String {
    text.trim().trim_matches('"').trim().to_string()
}
