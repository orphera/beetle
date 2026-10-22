//! Work that runs off the UI thread (INV-5): adding songs, installing,
//! packing, downloads and scans. Each task reports progress over a channel
//! and ends with one message for the status bar.

use crate::josa::{quoted_eul, quoted_eun};
use bms_package_manager::{
    AddError, AddOptions, AddProgress, AddReport, AddSource, Added, BgaPackMode, FolderMode,
    PackOptions, PackProfile, PackSource, PackageManager, PackageManagerError, SkipReason, Skipped,
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

/// What dropping or adding `path` does, as a task title; `None` when the path
/// is not something that can be added.
pub fn add_title(path: &Path) -> Result<String, String> {
    let name = file_name(path);
    match AddSource::of(path) {
        Some(AddSource::Folder) => Ok(format!("'{name}' 폴더에서 곡 추가하는 중")),
        Some(AddSource::Package | AddSource::BgaCompanion) => Ok(format!("'{name}' 설치하는 중")),
        Some(AddSource::Delta) => Ok(format!("'{name}' 업데이트 적용하는 중")),
        Some(AddSource::Archive) => Ok(format!("'{name}' 압축을 풀어 곡 추가하는 중")),
        None if !path.exists() => Err(format!(
            "{} 찾을 수 없어요",
            quoted_eul(&path.display().to_string())
        )),
        None => Err(format!(
            "{} 추가할 수 없는 파일이에요. 곡 폴더, 압축 파일(.zip/.rar/.7z), .bmsp 파일을 넣어 주세요.",
            quoted_eun(&name)
        )),
    }
}

/// Adds whatever `path` is: a song folder (or a folder of song folders), an
/// archive of one, a `.bmsp` package or a `.bmdp` update. Folders are copied
/// into the store, as this tool always has.
pub fn add_path(root: PathBuf, path: PathBuf, r: &Reporter) -> Result<String, String> {
    let mut mgr = manager(&root)?;
    let options = AddOptions {
        folders: FolderMode::Import,
        profile: PackProfile::Classic,
    };
    let result =
        bms_package_manager::add_path(&mut mgr, &path, &options, Some(&r.cancel), |p| match p {
            AddProgress::Unpacking { name } => r.progress("압축 푸는 중...", 0, 0, name),
            AddProgress::Song { index, total, name } => r.progress(
                &format!("{total}곡 중 {}번째 추가하는 중", index + 1),
                index,
                total,
                name,
            ),
            AddProgress::Step {
                phase,
                current,
                total,
                detail,
            } => r.progress(phase, current, total, detail),
        });
    match result {
        Ok(report) => add_message(&report),
        Err(AddError::Unsupported) => Err(add_title(&path).err().unwrap_or_default()),
        Err(AddError::Cancelled(report)) if report.added() > 0 => {
            Err(format!("{CANCELLED} ({}곡은 추가됨)", report.added()))
        }
        Err(AddError::Cancelled(_)) => Err(CANCELLED.to_string()),
        Err(AddError::Library(e)) => Err(format!("곡 폴더 목록을 저장할 수 없어요: {e}")),
    }
}

/// The status bar line for a finished add.
fn add_message(report: &AddReport) -> Result<String, String> {
    let added = report.installed.len();
    let total = added + report.skipped.len();
    match (report.installed.as_slice(), report.skipped.as_slice()) {
        ([one], []) => Ok(match one.how {
            Added::Imported => format!("{} 추가했어요", quoted_eul(&one.name)),
            Added::Installed => format!("{} 설치했어요", quoted_eul(&one.name)),
            Added::InstalledWithBga => {
                format!("{} 설치했어요 (배경 영상 포함)", quoted_eul(&one.name))
            }
            Added::Updated => format!("{} 업데이트했어요", quoted_eul(&one.name)),
            Added::Bga => format!("'{}'에 배경 영상을 설치했어요", one.id),
        }),
        ([], [one]) => Err(skip_message(one)),
        _ if added == total => Ok(format!("{added}곡을 모두 추가했어요")),
        _ => Ok(format!(
            "{total}곡 중 {added}곡을 추가했어요 (나머지는 읽을 수 없었어요)"
        )),
    }
}

fn skip_message(skip: &Skipped) -> String {
    match &skip.reason {
        SkipReason::NoCharts => format!(
            "'{}'에서 BMS 채보 파일(.bms, .bme 등)을 찾지 못했어요",
            skip.name
        ),
        SkipReason::Unpack(e) => format!("압축을 풀 수 없어요: {e}"),
        SkipReason::AlreadyInstalled => format!("{} 이미 설치되어 있어요", quoted_eun(&skip.name)),
        SkipReason::AlreadyLinked => format!("{} 이미 연결되어 있어요", quoted_eun(&skip.name)),
        SkipReason::Failed(e) => format!("곡 추가 실패: {e}"),
    }
}

/// Packs each song folder under `folder` into `.bmsp` files next to it. A
/// zip is read in place and a rar / 7z unpacked to a temp folder first; each
/// song's package goes next to the archive.
pub fn pack(
    folder: PathBuf,
    turbo: bool,
    bga: BgaPackMode,
    r: &Reporter,
) -> Result<String, String> {
    if bms_package_manager::is_archive_path(&folder) {
        r.progress("압축 푸는 중...", 0, 1, &file_name(&folder));
    }
    let source = match PackSource::open_with_cancel(&folder, Some(&r.cancel)) {
        Ok(source) => source,
        Err(PackageManagerError::Cancelled) => return Err(CANCELLED.to_string()),
        Err(e) => return Err(failed("압축 풀기", e)),
    };
    let roots = source.songs();
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
    // One song: the file goes next to the folder. Several: inside the folder.
    // An archive's: next to the archive.
    let out_dir = source.out_dir(&roots);
    let total = roots.len();
    let mut made = Vec::new();
    for (i, song) in roots.iter().enumerate() {
        if r.cancelled() {
            return Err(CANCELLED.to_string());
        }
        let name = song.name.clone();
        let out = out_dir.join(format!("{name}.bmsp"));
        let result = if total == 1 {
            source.pack(song, options, Some(&r.cancel), r.callback())
        } else {
            r.progress(
                &format!("{total}곡 중 {}번째 만드는 중", i + 1),
                i,
                total,
                &name,
            );
            source.pack(song, options, Some(&r.cancel), |_, _, _, _| {})
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
        [one] => Ok(format!(
            "{} 만들었어요",
            quoted_eul(&one.display().to_string())
        )),
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
        "업데이트 파일 {} 만들었어요",
        quoted_eul(&out.display().to_string())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bms_package_manager::InstalledSong;

    fn song(name: &str, how: Added) -> InstalledSong {
        InstalledSong {
            id: format!("id.{name}"),
            name: name.to_string(),
            how,
        }
    }

    fn skipped(name: &str, reason: SkipReason) -> Skipped {
        Skipped {
            name: name.to_string(),
            reason,
        }
    }

    #[test]
    fn add_messages_follow_the_report() {
        let one = |how| AddReport {
            installed: vec![song("곡", how)],
            ..Default::default()
        };
        assert_eq!(
            add_message(&one(Added::Imported)).unwrap(),
            "'곡'을 추가했어요"
        );
        assert_eq!(
            add_message(&one(Added::InstalledWithBga)).unwrap(),
            "'곡'을 설치했어요 (배경 영상 포함)"
        );
        assert_eq!(
            add_message(&one(Added::Bga)).unwrap(),
            "'id.곡'에 배경 영상을 설치했어요"
        );

        let none = AddReport {
            skipped: vec![skipped("pics", SkipReason::NoCharts)],
            ..Default::default()
        };
        assert!(add_message(&none)
            .unwrap_err()
            .contains("'pics'에서 BMS 채보"));

        let some = AddReport {
            installed: vec![song("a", Added::Imported), song("b", Added::Imported)],
            skipped: vec![skipped("c", SkipReason::AlreadyInstalled)],
            ..Default::default()
        };
        assert_eq!(
            add_message(&some).unwrap(),
            "3곡 중 2곡을 추가했어요 (나머지는 읽을 수 없었어요)"
        );
    }
}
