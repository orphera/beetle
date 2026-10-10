//! Work that runs off the UI thread (INV-5): adding songs, installing,
//! packing, downloads and scans. Each task reports progress over a channel
//! and ends with one message for the status bar.

use bms_package_manager::{
    BgaPackMode, PackOptions, PackProfile, PackageManager, PackageManagerError,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

pub enum TaskMessage {
    Progress {
        phase: String,
        current: usize,
        total: usize,
        detail: String,
    },
    Done(Result<String, String>),
}

/// What finishing a task changes besides the song list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Other,
    /// Added a chart or a song: the collection index is now out of date.
    Adds,
    /// Rescanned: the collection index is current.
    TableScan,
    /// Installed a difficulty table.
    TableAdded,
    /// Read a chart's IR page for its download links. Finishing opens the
    /// download dialog with them.
    IrLookup,
}

/// A task running now.
pub struct BgTask {
    pub title: String,
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub detail: String,
    pub kind: TaskKind,
    pub cancel: Arc<AtomicBool>,
    pub receiver: Receiver<TaskMessage>,
}

/// The task's side of the channel.
#[derive(Clone)]
pub struct Reporter {
    tx: Sender<TaskMessage>,
    pub cancel: Arc<AtomicBool>,
}

impl Reporter {
    pub fn progress(&self, phase: &str, current: usize, total: usize, detail: &str) {
        let _ = self.tx.send(TaskMessage::Progress {
            phase: phase.to_string(),
            current,
            total,
            detail: detail.to_string(),
        });
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// A progress callback for the package manager's `*_with_progress` calls.
    fn callback(&self) -> impl FnMut(&str, usize, usize, &str) {
        let me = self.clone();
        move |phase, current, total, detail| me.progress(phase, current, total, detail)
    }
}

/// Starts `work` on a new thread.
pub fn spawn<F>(title: String, phase: &str, kind: TaskKind, work: F) -> BgTask
where
    F: FnOnce(&Reporter) -> Result<String, String> + Send + 'static,
{
    let (tx, receiver) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let reporter = Reporter {
        tx: tx.clone(),
        cancel: cancel.clone(),
    };
    thread::spawn(move || {
        let result = work(&reporter);
        let _ = tx.send(TaskMessage::Done(result));
    });
    BgTask {
        title,
        phase: phase.to_string(),
        current: 0,
        total: 0,
        detail: String::new(),
        kind,
        cancel,
        receiver,
    }
}

fn manager(root: &Path) -> Result<PackageManager, String> {
    PackageManager::new(root).map_err(|e| format!("곡 저장소를 열 수 없어요: {e}"))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string()
}

const CANCELLED: &str = "취소했어요";

fn failed(what: &str, e: PackageManagerError) -> String {
    match e {
        PackageManagerError::Cancelled => CANCELLED.to_string(),
        e => format!("{what} 실패: {e}"),
    }
}

/// Archives that can hold a song folder.
pub fn is_archive(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(ext.as_str(), "zip" | "rar" | "7z")
}

/// What dropping or adding `path` does, as a task title; `None` when the path
/// is not something that can be added.
pub fn add_title(path: &Path) -> Result<String, String> {
    let name = file_name(path);
    let lower = name.to_ascii_lowercase();
    if path.is_dir() {
        Ok(format!("'{name}' 폴더에서 곡 추가하는 중"))
    } else if !path.exists() {
        Err(format!("'{}'을(를) 찾을 수 없어요", path.display()))
    } else if lower.ends_with(".bmsp") {
        Ok(format!("'{name}' 설치하는 중"))
    } else if lower.ends_with(".bmdp") {
        Ok(format!("'{name}' 업데이트 적용하는 중"))
    } else if is_archive(path) {
        Ok(format!("'{name}' 압축을 풀어 곡 추가하는 중"))
    } else {
        Err(format!(
            "'{name}'은(는) 추가할 수 없는 파일이에요. 곡 폴더, 압축 파일(.zip/.rar/.7z), .bmsp 파일을 넣어 주세요."
        ))
    }
}

/// Adds whatever `path` is: a song folder (or a folder of song folders), an
/// archive of one, a `.bmsp` package or a `.bmdp` update.
pub fn add_path(root: PathBuf, path: PathBuf, r: &Reporter) -> Result<String, String> {
    let lower = file_name(&path).to_ascii_lowercase();
    if path.is_dir() {
        import_folders(&root, &path, r)
    } else if lower.ends_with(".bga.bmsp") {
        let id = manager(&root)?
            .install_bga_companion(&path)
            .map_err(|e| failed("배경 영상 설치", e))?;
        Ok(format!("'{id}'에 배경 영상을 설치했어요"))
    } else if lower.ends_with(".bmsp") {
        install_bmsp(&root, &path, r)
    } else if lower.ends_with(".bmdp") {
        let installed = manager(&root)?
            .apply_delta(&path)
            .map_err(|e| failed("업데이트 적용", e))?;
        Ok(format!("'{}'을(를) 업데이트했어요", installed.name))
    } else if is_archive(&path) {
        let scratch = std::env::temp_dir().join(format!("bpm-gui-add-{}", std::process::id()));
        let _ = fs::remove_dir_all(&scratch);
        r.progress("압축 푸는 중...", 0, 0, &file_name(&path));
        let result = bms_package_manager::table_ops::unpack_body(&path, &scratch)
            .map_err(|e| format!("압축을 풀 수 없어요: {e}"))
            .and_then(|folder| import_folders(&root, &folder, r));
        let _ = fs::remove_dir_all(&scratch);
        result
    } else {
        Err(add_title(&path).err().unwrap_or_default())
    }
}

fn install_bmsp(root: &Path, path: &Path, r: &Reporter) -> Result<String, String> {
    let mut mgr = manager(root)?;
    let installed = mgr
        .install_with_progress(path, Some(&r.cancel), r.callback())
        .map_err(|e| failed("설치", e))?;
    // A background video package next to it is installed too.
    let companion = path.with_file_name(format!(
        "{}.bga.bmsp",
        file_name(path).trim_end_matches(".bmsp")
    ));
    let mut extra = "";
    if companion.exists() && mgr.install_bga_companion(&companion).is_ok() {
        extra = " (배경 영상 포함)";
    }
    Ok(format!("'{}'을(를) 설치했어요{extra}", installed.name))
}

/// Imports every song folder under `folder`.
fn import_folders(root: &Path, folder: &Path, r: &Reporter) -> Result<String, String> {
    let roots = bms_package_manager::find_bms_song_roots(folder);
    if roots.is_empty() {
        return Err(format!(
            "'{}'에서 BMS 채보 파일(.bms, .bme 등)을 찾지 못했어요",
            folder.display()
        ));
    }
    let mut mgr = manager(root)?;
    if roots.len() == 1 {
        let installed = mgr
            .import_folder_with_progress(&roots[0], None, Some(&r.cancel), r.callback())
            .map_err(|e| failed("곡 추가", e))?;
        return Ok(format!("'{}'을(를) 추가했어요", installed.name));
    }
    let total = roots.len();
    let mut added = 0;
    for (i, song) in roots.iter().enumerate() {
        if r.cancelled() {
            return Err(format!("취소했어요 ({added}곡은 추가됨)"));
        }
        let name = file_name(song);
        r.progress(
            &format!("{total}곡 중 {}번째 추가하는 중", i + 1),
            i,
            total,
            &name,
        );
        match mgr.import_folder_with_progress(song, None, Some(&r.cancel), |_, _, _, _| {}) {
            Ok(_) => added += 1,
            Err(PackageManagerError::Cancelled) => {
                return Err(format!("취소했어요 ({added}곡은 추가됨)"))
            }
            Err(_) => {}
        }
    }
    if added == total {
        Ok(format!("{added}곡을 모두 추가했어요"))
    } else {
        Ok(format!(
            "{total}곡 중 {added}곡을 추가했어요 (나머지는 읽을 수 없었어요)"
        ))
    }
}

/// Packs each song folder under `folder` into `.bmsp` files next to it.
pub fn pack(
    root: PathBuf,
    folder: PathBuf,
    turbo: bool,
    bga: BgaPackMode,
    r: &Reporter,
) -> Result<String, String> {
    let roots = bms_package_manager::find_bms_song_roots(&folder);
    if roots.is_empty() {
        return Err(format!(
            "'{}'에서 BMS 채보 파일을 찾지 못했어요",
            folder.display()
        ));
    }
    let profile = if turbo {
        PackProfile::Turbo
    } else {
        PackProfile::Classic
    };
    let options = PackOptions::new(profile, bga);
    let mgr = manager(&root)?;
    // One song: the file goes next to the folder. Several: inside the folder.
    let out_dir = if roots.len() == 1 && roots[0] == folder {
        folder.parent().unwrap_or(&folder).to_path_buf()
    } else {
        folder.clone()
    };
    let total = roots.len();
    let mut made = Vec::new();
    for (i, song) in roots.iter().enumerate() {
        if r.cancelled() {
            return Err(CANCELLED.to_string());
        }
        let name = file_name(song);
        let out = out_dir.join(format!("{name}.bmsp"));
        let result = if total == 1 {
            mgr.pack_folder_advanced_with_progress(
                song,
                None,
                options,
                Some(&r.cancel),
                r.callback(),
            )
        } else {
            r.progress(
                &format!("{total}곡 중 {}번째 만드는 중", i + 1),
                i,
                total,
                &name,
            );
            mgr.pack_folder_advanced_with_progress(
                song,
                None,
                options,
                Some(&r.cancel),
                |_, _, _, _| {},
            )
        };
        match result {
            Ok(output) => {
                fs::write(&out, &output.base_package)
                    .map_err(|e| format!("'{}'에 저장할 수 없어요: {e}", out.display()))?;
                if let Some(bga_bytes) = output.bga_package {
                    let _ = fs::write(out.with_extension("bga.bmsp"), bga_bytes);
                }
                made.push(out);
            }
            Err(PackageManagerError::Cancelled) => return Err(CANCELLED.to_string()),
            Err(e) if total == 1 => return Err(failed("패키지 만들기", e)),
            Err(_) => {}
        }
    }
    match made.as_slice() {
        [one] => Ok(format!("'{}'을(를) 만들었어요", one.display())),
        _ => Ok(format!(
            "{total}곡 중 {}곡의 패키지를 '{}'에 만들었어요",
            made.len(),
            out_dir.display()
        )),
    }
}

/// Writes an update file that turns the `base` folder into the `target` folder.
pub fn create_delta(base: PathBuf, target: PathBuf) -> Result<String, String> {
    let bytes = bms_package_manager::PackageUpdater::create_delta_between_paths(&base, &target)
        .map_err(|e| format!("업데이트 파일 만들기 실패: {e}"))?;
    let out = target
        .parent()
        .unwrap_or(&target)
        .join(format!("{}.bmdp", file_name(&target)));
    fs::write(&out, bytes).map_err(|e| format!("'{}'에 저장할 수 없어요: {e}", out.display()))?;
    Ok(format!(
        "업데이트 파일 '{}'을(를) 만들었어요",
        out.display()
    ))
}
