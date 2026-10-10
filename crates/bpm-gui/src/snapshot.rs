//! Renders the window's screens to BMP files in the folder named by
//! `BPM_GUI_SNAPSHOT_DIR`, for a visual check without opening a window.
//! Run it with `cargo test -p bpm-gui snapshot -- --ignored`. The Tables
//! screens read the tables and the collection index of the folder named by
//! `BPM_GUI_SNAPSHOT_CWD` (default: the repository root).

use crate::dialogs::{Dialog, DialogKind};
use crate::tables_tab::TablesTab;
use crate::ui::{ActiveTab, Frame, StatusKind, TaskProgressInfo};
use crate::widgets::{GuiRenderer, ListView};
use bms_package_manager::{BgaPackMode, BgaStatus, PackageRecord, PackageStateRecord};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn package(name: &str, author: &str, bga: BgaStatus, versions: usize) -> PackageRecord {
    let mut states = BTreeMap::new();
    for v in 0..versions {
        states.insert(
            format!("{v}a1b2c3d4e5f6a7b8c9d0"),
            PackageStateRecord {
                path: "x".into(),
                installed_at: "0".into(),
            },
        );
    }
    PackageRecord {
        id: name.to_lowercase().replace(' ', "-"),
        name: name.into(),
        author: Some(author.into()),
        active_state: "0a1b2c3d4e5f6a7b8c9d0".into(),
        bga_status: bga,
        bga_companion_path: None,
        state_hashes: states,
    }
}

struct Scene<'a> {
    tab: ActiveTab,
    packages: &'a [&'a PackageRecord],
    dialog: Option<Dialog>,
    task: Option<TaskProgressInfo<'a>>,
    status: (&'a str, StatusKind),
    drop: bool,
    library: Vec<String>,
}

impl<'a> Scene<'a> {
    fn new(tab: ActiveTab, packages: &'a [&'a PackageRecord]) -> Self {
        Self {
            tab,
            packages,
            dialog: None,
            task: None,
            status: (
                "준비됐어요. 처음이라면 오른쪽 위 '도움말'을 눌러 보세요.",
                StatusKind::Info,
            ),
            drop: false,
            library: Vec::new(),
        }
    }
}

fn render(r: &mut GuiRenderer, tables: &mut TablesTab, scene: Scene, out: &Path) {
    let mut installed = ListView::default();
    let dialog = scene.dialog.as_ref().map(|d| d.view(&scene.library));
    r.render(Frame {
        tab: scene.tab,
        packages: scene.packages,
        installed_total: scene.packages.len(),
        installed: &mut installed,
        preview: None,
        search: "",
        search_active: false,
        status: scene.status.0,
        status_kind: scene.status.1,
        dialog: dialog.as_ref(),
        task: scene.task,
        tables,
        drop_hint: scene.drop,
    });
    write_bmp(r, out);
}

#[test]
#[ignore]
fn snapshot_screens() {
    let Ok(dir) = std::env::var("BPM_GUI_SNAPSHOT_DIR") else {
        return;
    };
    let dir = Path::new(&dir).to_path_buf();
    let cwd = std::env::var("BPM_GUI_SNAPSHOT_CWD").unwrap_or_else(|_| "../..".into());
    std::env::set_current_dir(cwd).expect("snapshot cwd");

    let owned = [
        package("FREEDOM DiVE", "xi", BgaStatus::Embedded, 2),
        package("Air", "Ryu*", BgaStatus::Companion, 1),
        package("冥", "Amane", BgaStatus::None, 1),
        package("Blue Zenith", "xi", BgaStatus::None, 1),
    ];
    let packages: Vec<&PackageRecord> = owned.iter().collect();

    let mut r = GuiRenderer::new(1080, 740).expect("pixmap");
    let mut tables = TablesTab::load();
    let shot = |name: &str| dir.join(format!("{name}.bmp"));

    render(
        &mut r,
        &mut tables,
        Scene::new(ActiveTab::Installed, &[]),
        &shot("01-installed-empty"),
    );
    render(
        &mut r,
        &mut tables,
        Scene::new(ActiveTab::Installed, &packages),
        &shot("02-installed"),
    );
    render(
        &mut r,
        &mut tables,
        Scene::new(ActiveTab::Tables, &packages),
        &shot("05-tables"),
    );

    let with_dialog = |kind: DialogKind| {
        let mut scene = Scene::new(ActiveTab::Installed, &packages);
        scene.dialog = Some(Dialog::new(kind));
        scene
    };
    render(
        &mut r,
        &mut tables,
        with_dialog(DialogKind::Add),
        &shot("06-dialog-add"),
    );
    render(
        &mut r,
        &mut tables,
        with_dialog(DialogKind::Help),
        &shot("07-dialog-help"),
    );
    render(
        &mut r,
        &mut tables,
        with_dialog(DialogKind::Pack {
            turbo: false,
            bga: BgaPackMode::Split,
        }),
        &shot("08-dialog-pack"),
    );
    render(
        &mut r,
        &mut tables,
        with_dialog(DialogKind::ConfirmUninstall {
            id: "freedom-dive".into(),
            name: "FREEDOM DiVE".into(),
            state: None,
        }),
        &shot("09-dialog-uninstall"),
    );
    let mut scene = with_dialog(DialogKind::Library);
    scene.library = vec!["D:\\BMS\\songs".into(), "E:\\old\\bms".into()];
    render(&mut r, &mut tables, scene, &shot("10-dialog-library"));

    let mut scene = Scene::new(ActiveTab::Installed, &packages);
    scene.task = Some(TaskProgressInfo {
        title: "'Sample Song' 내려받는 중",
        phase: "내려받는 중...",
        current: 9_000_000,
        total: 23_456_789,
        detail: "8.6 / 22.4 MB",
        frame: 0,
        cancelling: false,
    });
    render(&mut r, &mut tables, scene, &shot("11-task"));

    let mut scene = Scene::new(ActiveTab::Installed, &packages);
    scene.drop = true;
    render(&mut r, &mut tables, scene, &shot("12-drop"));

    render(
        &mut r,
        &mut tables,
        with_dialog(DialogKind::TableFetchIr {
            title: "Qualia [Quantia]".into(),
            folder: "songs/Satellite/1475".into(),
            links: vec![
                (
                    "body",
                    "https://web.archive.org/web/20161001063700/https://dl.dropboxusercontent.com/u/93651732/bms/junk_qualia.zip".into(),
                ),
                ("diff", "https://bms.hexlataia.xyz/mirror/gnqg-upload/02646.zip".into()),
            ],
            others: 1,
        }),
        &shot("14-dialog-ir-fetch"),
    );

    let mut scene = Scene::new(ActiveTab::Installed, &packages);
    scene.status = ("'FREEDOM DiVE'을(를) 삭제했어요", StatusKind::Success);
    scene.dialog = Some(Dialog::new(DialogKind::Advanced));
    render(&mut r, &mut tables, scene, &shot("13-dialog-advanced"));
}

fn write_bmp(renderer: &GuiRenderer, path: &Path) {
    let (w, h) = (renderer.pixmap.width(), renderer.pixmap.height());
    let data = renderer.pixmap.data();
    let mut out = Vec::with_capacity(54 + data.len());
    let image_size = w * h * 4;
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(54 + image_size).to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for px in data.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    fs::write(path, out).expect("write snapshot");
}
