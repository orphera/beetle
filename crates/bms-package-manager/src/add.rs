//! Adding songs from whatever the user hands over: a song folder (or a folder
//! of song folders), an archive of one, a `.bmsp` package, a `.bga.bmsp`
//! background video or a `.bmdp` update. The game and `bpm-gui` both call
//! [`add_path`]; each turns the [`AddReport`] into sentences of its own.

use crate::error::PackageManagerError;
use crate::manager::PackageManager;
use crate::pack::{find_bms_song_roots, PackProfile};
use crate::same_song::{self, Incoming, KnownPlace, Verdict};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// What a path is, as far as adding goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddSource {
    Folder,
    Package,
    BgaCompanion,
    Delta,
    Archive,
}

impl AddSource {
    /// `None` when `path` is missing or not something that can be added.
    pub fn of(path: &Path) -> Option<Self> {
        let lower = file_name(path).to_ascii_lowercase();
        if path.is_dir() {
            Some(Self::Folder)
        } else if !path.is_file() {
            None
        } else if lower.ends_with(".bga.bmsp") {
            Some(Self::BgaCompanion)
        } else if lower.ends_with(".bmsp") {
            Some(Self::Package)
        } else if lower.ends_with(".bmdp") {
            Some(Self::Delta)
        } else if is_archive(path) {
            Some(Self::Archive)
        } else {
            None
        }
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

/// What adding a folder does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderMode {
    /// Pack each song folder and install the package.
    Import,
    /// Leave the folder where it is and list it in this `library.dat`.
    Link(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddOptions {
    pub folders: FolderMode,
    /// The profile song folders (and unpacked archives) are packed with.
    pub profile: PackProfile,
}

/// Progress of [`add_path`].
#[derive(Debug, Clone, Copy)]
pub enum AddProgress<'a> {
    /// Unpacking the archive `name`.
    Unpacking { name: &'a str },
    /// Starting song `index` (from 0) of `total`. Steps inside it are not reported.
    Song {
        index: usize,
        total: usize,
        name: &'a str,
    },
    /// A step of the one package being made or installed, as the package
    /// manager's `*_with_progress` calls report it.
    Step {
        phase: &'a str,
        current: usize,
        total: usize,
        detail: &'a str,
    },
}

/// How a song came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Added {
    /// Packed from a folder and installed.
    Imported,
    /// Installed from a package file.
    Installed,
    /// Installed from a package file, with the background video next to it.
    InstalledWithBga,
    /// An update applied to an installed song.
    Updated,
    /// A background video added to an installed song.
    Bga,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledSong {
    pub id: String,
    pub name: String,
    pub how: Added,
}

#[derive(Debug)]
pub enum SkipReason {
    /// No chart file (`.bms`, `.bme`, ...) anywhere in it.
    NoCharts,
    /// The archive could not be unpacked.
    Unpack(String),
    AlreadyInstalled,
    /// Every chart and the key sounds are already in the library, as song
    /// `song`; the user chose not to add it again.
    AlreadyThere {
        song: String,
    },
    /// A key sound set of its own for song `song`; the user chose not to add it.
    Declined {
        song: String,
    },
    /// The folder is already in `library.dat`.
    AlreadyLinked,
    Failed(PackageManagerError),
}

#[derive(Debug)]
pub struct Skipped {
    /// The file or folder name.
    pub name: String,
    pub reason: SkipReason,
}

#[derive(Debug, Default)]
pub struct AddReport {
    pub installed: Vec<InstalledSong>,
    /// Folders added to `library.dat`, in the form it stores them.
    pub linked: Vec<String>,
    pub skipped: Vec<Skipped>,
    /// How what came in relates to the library (`same_song::judge`), by name,
    /// when it joined a known song or is a candidate for one.
    pub verdicts: Vec<(String, Verdict)>,
    /// Packages removed because the user replaced their key sound set.
    pub replaced: Vec<String>,
}

impl AddReport {
    /// Songs and folders that came in.
    pub fn added(&self) -> usize {
        self.installed.len() + self.linked.len()
    }

    fn skip(&mut self, name: &str, reason: SkipReason) {
        self.skipped.push(Skipped {
            name: name.to_string(),
            reason,
        });
    }
}

#[derive(Debug)]
pub enum AddError {
    /// `path` is missing or not something [`AddSource::of`] knows.
    Unsupported,
    /// Stopped by the cancel flag; what was added before stays.
    Cancelled(AddReport),
    /// `library.dat` could not be written.
    Library(std::io::Error),
}

/// What the user answers about a song already in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Add,
    Skip,
    /// Add it, and remove the installed packages that held the old key sound
    /// set (`Verdict::OtherSet::replaceable`).
    Replace,
}

/// What [`add_path_judged`] checks songs against, and how it asks.
pub struct Judge<'a> {
    pub known: &'a [KnownPlace],
    /// Called for `Verdict::AlreadyThere` and `Verdict::OtherSet`, with the
    /// name of what is being added. Blocks until the user answers.
    pub ask: &'a mut dyn FnMut(&str, &Verdict) -> Answer,
}

/// Adds `path`. Anything that could not be added is listed in the report's
/// `skipped` rather than failing the whole call.
pub fn add_path<F>(
    mgr: &mut PackageManager,
    path: &Path,
    options: &AddOptions,
    cancel: Option<&AtomicBool>,
    on_progress: F,
) -> Result<AddReport, AddError>
where
    F: FnMut(AddProgress<'_>),
{
    let mut add = |_: &str, _: &Verdict| Answer::Add;
    let judge = Judge {
        known: &[],
        ask: &mut add,
    };
    add_path_judged(mgr, path, options, judge, cancel, on_progress)
}

/// [`add_path`], checking each song folder and package against the library
/// first (B18, `same_song::judge`). Linked folders are not checked: they stay
/// the user's own folders.
pub fn add_path_judged<F>(
    mgr: &mut PackageManager,
    path: &Path,
    options: &AddOptions,
    mut judge: Judge<'_>,
    cancel: Option<&AtomicBool>,
    mut on_progress: F,
) -> Result<AddReport, AddError>
where
    F: FnMut(AddProgress<'_>),
{
    let source = AddSource::of(path).ok_or(AddError::Unsupported)?;
    let name = file_name(path);
    let mut report = AddReport::default();
    match source {
        AddSource::Folder => match &options.folders {
            FolderMode::Import => import_folders(
                mgr,
                path,
                &name,
                options.profile,
                &mut judge,
                cancel,
                &mut on_progress,
                &mut report,
            )?,
            FolderMode::Link(library) => link_folder(library, path, &name, &mut report)?,
        },
        AddSource::BgaCompanion => match mgr.install_bga_companion(path) {
            Ok(id) => report.installed.push(InstalledSong {
                name: id.clone(),
                id,
                how: Added::Bga,
            }),
            Err(e) => report.skip(&name, SkipReason::Failed(e)),
        },
        AddSource::Package => {
            let incoming = Incoming::package(path);
            let Some(answer) = decide(&mut judge, incoming.as_ref(), &name, &mut report) else {
                return Ok(report);
            };
            let mut step = steps(&mut on_progress);
            match mgr.install_with_progress(path, cancel, &mut step) {
                Ok(song) => {
                    // A background video package next to it is installed too.
                    let companion =
                        path.with_file_name(format!("{}.bga.bmsp", name.trim_end_matches(".bmsp")));
                    let how = if companion.exists() && mgr.install_bga_companion(&companion).is_ok()
                    {
                        Added::InstalledWithBga
                    } else {
                        Added::Installed
                    };
                    report.installed.push(InstalledSong {
                        id: song.id,
                        name: song.name,
                        how,
                    });
                    replace(mgr, answer, &mut report);
                }
                Err(e) => skip_error(&mut report, &name, e)?,
            }
        }
        AddSource::Delta => match mgr.apply_delta(path) {
            Ok(song) => report.installed.push(InstalledSong {
                id: song.id,
                name: song.name,
                how: Added::Updated,
            }),
            Err(e) => report.skip(&name, SkipReason::Failed(e)),
        },
        AddSource::Archive => {
            let scratch = scratch_dir();
            let _ = fs::remove_dir_all(&scratch);
            on_progress(AddProgress::Unpacking { name: &name });
            let result = match crate::table_ops::unpack_body_with_cancel(path, &scratch, cancel) {
                Ok(folder) => import_folders(
                    mgr,
                    &folder,
                    &name,
                    options.profile,
                    &mut judge,
                    cancel,
                    &mut on_progress,
                    &mut report,
                ),
                Err(PackageManagerError::Cancelled) => {
                    Err(AddError::Cancelled(std::mem::take(&mut report)))
                }
                Err(e) => {
                    report.skip(&name, SkipReason::Unpack(e.to_string()));
                    Ok(())
                }
            };
            let _ = fs::remove_dir_all(&scratch);
            result?;
        }
    }
    Ok(report)
}

/// What to do with something being added, from its verdict: `None` to leave
/// it out (recorded as skipped), else the answer and the packages a replace
/// removes. Verdicts that join or may join a known song are recorded.
fn decide(
    judge: &mut Judge<'_>,
    incoming: Option<&Incoming>,
    name: &str,
    report: &mut AddReport,
) -> Option<Vec<String>> {
    let (Some(incoming), false) = (incoming, judge.known.is_empty()) else {
        return Some(Vec::new());
    };
    let verdict = same_song::judge(incoming, judge.known);
    let answer = match &verdict {
        Verdict::AlreadyThere { .. } | Verdict::OtherSet { .. } => (judge.ask)(name, &verdict),
        _ => Answer::Add,
    };
    let replaceable = match (answer, &verdict) {
        (Answer::Skip, Verdict::AlreadyThere { song }) => {
            report.skip(name, SkipReason::AlreadyThere { song: song.clone() });
            return None;
        }
        (Answer::Skip, Verdict::OtherSet { song, .. }) => {
            report.skip(name, SkipReason::Declined { song: song.clone() });
            return None;
        }
        (Answer::Replace, Verdict::OtherSet { replaceable, .. }) => replaceable.clone(),
        _ => Vec::new(),
    };
    if verdict != Verdict::NewSong {
        report.verdicts.push((name.to_string(), verdict));
    }
    Some(replaceable)
}

/// After a replace: uninstalls every version of the packages that held the
/// old key sound set.
fn replace(mgr: &mut PackageManager, packages: Vec<String>, report: &mut AddReport) {
    for id in packages {
        let hashes: Vec<String> = mgr
            .registry()
            .list_packages()
            .into_iter()
            .find(|r| r.id == id)
            .map(|r| r.state_hashes.keys().cloned().collect())
            .unwrap_or_default();
        if hashes.iter().all(|hash| mgr.uninstall(&id, hash).is_ok()) {
            report.replaced.push(id);
        }
    }
}

/// A folder of its own under the temp directory for unpacking one archive.
fn scratch_dir() -> PathBuf {
    use std::sync::atomic::AtomicUsize;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "bpm-add-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn steps<'f, F: FnMut(AddProgress<'_>)>(
    on_progress: &'f mut F,
) -> impl FnMut(&str, usize, usize, &str) + 'f {
    move |phase, current, total, detail| {
        on_progress(AddProgress::Step {
            phase,
            current,
            total,
            detail,
        })
    }
}

/// Records `e` as a skip, or stops on cancellation.
fn skip_error(report: &mut AddReport, name: &str, e: PackageManagerError) -> Result<(), AddError> {
    match e {
        PackageManagerError::Cancelled => Err(AddError::Cancelled(std::mem::take(report))),
        PackageManagerError::AlreadyInstalled { .. } => {
            report.skip(name, SkipReason::AlreadyInstalled);
            Ok(())
        }
        e => {
            report.skip(name, SkipReason::Failed(e));
            Ok(())
        }
    }
}

/// Packs and installs every song folder under `folder`. `name` stands for
/// `folder` when nothing in it is a song.
#[allow(clippy::too_many_arguments)]
fn import_folders<F: FnMut(AddProgress<'_>)>(
    mgr: &mut PackageManager,
    folder: &Path,
    name: &str,
    profile: PackProfile,
    judge: &mut Judge<'_>,
    cancel: Option<&AtomicBool>,
    on_progress: &mut F,
    report: &mut AddReport,
) -> Result<(), AddError> {
    let roots = find_bms_song_roots(folder);
    if roots.is_empty() {
        report.skip(name, SkipReason::NoCharts);
        return Ok(());
    }
    let total = roots.len();
    for (index, song) in roots.iter().enumerate() {
        if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) {
            return Err(AddError::Cancelled(std::mem::take(report)));
        }
        let song_name = file_name(song);
        let incoming = Incoming::folder(song);
        let Some(answer) = decide(judge, Some(&incoming), &song_name, report) else {
            continue;
        };
        let result = if total == 1 {
            import_one(mgr, song, profile, cancel, steps(on_progress))
        } else {
            on_progress(AddProgress::Song {
                index,
                total,
                name: &song_name,
            });
            import_one(mgr, song, profile, cancel, |_, _, _, _| {})
        };
        match result {
            Ok(installed) => {
                report.installed.push(InstalledSong {
                    id: installed.id,
                    name: installed.name,
                    how: Added::Imported,
                });
                replace(mgr, answer, report);
            }
            Err(e) => skip_error(report, &song_name, e)?,
        }
    }
    Ok(())
}

fn import_one<F: FnMut(&str, usize, usize, &str)>(
    mgr: &mut PackageManager,
    song: &Path,
    profile: PackProfile,
    cancel: Option<&AtomicBool>,
    mut on_progress: F,
) -> Result<crate::manager::InstalledPackage, PackageManagerError> {
    let bytes =
        mgr.pack_folder_profile_with_progress(song, None, profile, cancel, &mut on_progress)?;
    mgr.install_from_bytes_with_progress(bytes, cancel, on_progress)
}

/// Lists `folder` in `library` when it holds at least one song.
fn link_folder(
    library: &Path,
    folder: &Path,
    name: &str,
    report: &mut AddReport,
) -> Result<(), AddError> {
    if find_bms_song_roots(folder).is_empty() {
        report.skip(name, SkipReason::NoCharts);
        return Ok(());
    }
    let dir = match crate::library::absolute_dir(&folder.to_string_lossy()) {
        Ok(dir) => dir,
        Err(e) => {
            report.skip(
                name,
                SkipReason::Failed(PackageManagerError::StorageError(e)),
            );
            return Ok(());
        }
    };
    let mut list =
        beetle_core::LibraryPaths::parse(&fs::read_to_string(library).unwrap_or_default());
    if !list.add(&dir) {
        report.skip(name, SkipReason::AlreadyLinked);
        return Ok(());
    }
    beetle_core::write_atomic(library, list.serialize()).map_err(AddError::Library)?;
    report.linked.push(dir);
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bpm_add_{tag}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn song(dir: &Path, title: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("song.bms"),
            format!("#TITLE {title}\n#BPM 150\n#00111:01\n"),
        )
        .unwrap();
    }

    fn import() -> AddOptions {
        AddOptions {
            folders: FolderMode::Import,
            profile: PackProfile::Classic,
        }
    }

    fn add(mgr: &mut PackageManager, path: &Path, options: &AddOptions) -> AddReport {
        add_path(mgr, path, options, None, |_| {}).unwrap()
    }

    #[test]
    fn source_follows_the_name_and_kind() {
        let dir = temp("source");
        for name in [
            "a.bmsp",
            "a.BGA.bmsp",
            "a.bmdp",
            "a.ZIP",
            "a.rar",
            "a.7z",
            "a.txt",
        ] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        let of = |name: &str| AddSource::of(&dir.join(name));
        assert_eq!(of(""), Some(AddSource::Folder));
        assert_eq!(of("a.bmsp"), Some(AddSource::Package));
        assert_eq!(of("a.BGA.bmsp"), Some(AddSource::BgaCompanion));
        assert_eq!(of("a.bmdp"), Some(AddSource::Delta));
        assert_eq!(of("a.ZIP"), Some(AddSource::Archive));
        assert_eq!(of("a.rar"), Some(AddSource::Archive));
        assert_eq!(of("a.7z"), Some(AddSource::Archive));
        assert_eq!(of("a.txt"), None);
        assert_eq!(of("missing.bmsp"), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unsupported_path_is_an_error() {
        let dir = temp("unsupported");
        let mut mgr = PackageManager::new(dir.join("store")).unwrap();
        let result = add_path(&mut mgr, &dir.join("nope.bmsp"), &import(), None, |_| {});
        assert!(matches!(result, Err(AddError::Unsupported)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn folder_of_songs_imports_each_and_reports_progress_per_song() {
        let dir = temp("folders");
        let src = dir.join("src");
        song(&src.join("one"), "One");
        song(&src.join("two"), "Two");
        fs::create_dir_all(src.join("empty")).unwrap();
        let mut mgr = PackageManager::new(dir.join("store")).unwrap();
        let mut songs = Vec::new();
        let report = add_path(&mut mgr, &src, &import(), None, |p| {
            if let AddProgress::Song { index, total, name } = p {
                songs.push((index, total, name.to_string()));
            }
        })
        .unwrap();
        assert_eq!(report.installed.len(), 2);
        assert!(report.installed.iter().all(|s| s.how == Added::Imported));
        assert!(report.skipped.is_empty());
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].1, 2);
        assert_eq!(mgr.list_active_packages().len(), 2);

        // The same songs again are skipped as already installed.
        let again = add(&mut mgr, &src, &import());
        assert_eq!(again.added(), 0);
        assert_eq!(again.skipped.len(), 2);
        assert!(again
            .skipped
            .iter()
            .all(|s| matches!(s.reason, SkipReason::AlreadyInstalled)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn folder_without_charts_is_skipped() {
        let dir = temp("nocharts");
        let src = dir.join("pictures");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), b"x").unwrap();
        let mut mgr = PackageManager::new(dir.join("store")).unwrap();
        let report = add(&mut mgr, &src, &import());
        assert_eq!(report.added(), 0);
        assert_eq!(report.skipped[0].name, "pictures");
        assert!(matches!(report.skipped[0].reason, SkipReason::NoCharts));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn package_file_installs() {
        let dir = temp("package");
        let src = dir.join("Song");
        song(&src, "Song");
        let mut mgr = PackageManager::new(dir.join("store")).unwrap();
        let file = dir.join("Song.bmsp");
        fs::write(&file, mgr.pack_folder(&src, None).unwrap()).unwrap();
        let report = add(&mut mgr, &file, &import());
        assert_eq!(report.installed.len(), 1);
        assert_eq!(report.installed[0].how, Added::Installed);
        assert_eq!(report.installed[0].name, "Song");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn archive_is_unpacked_packed_with_the_profile_and_cleaned_up() {
        let dir = temp("archive");
        let src = dir.join("Zipped");
        song(&src, "Zipped");
        let archive = dir.join("Zipped.zip");
        {
            let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("Zipped/song.bms", opts).unwrap();
            std::io::Write::write_all(&mut zip, &fs::read(src.join("song.bms")).unwrap()).unwrap();
            zip.finish().unwrap();
        }
        let mut mgr = PackageManager::new(dir.join("store")).unwrap();
        let mut unpacked = false;
        let options = AddOptions {
            folders: FolderMode::Import,
            profile: PackProfile::Turbo,
        };
        let report = add_path(&mut mgr, &archive, &options, None, |p| {
            unpacked |= matches!(p, AddProgress::Unpacking { name } if name == "Zipped.zip");
        })
        .unwrap();
        assert!(unpacked);
        assert_eq!(report.installed.len(), 1, "{report:?}");
        assert_eq!(report.installed[0].how, Added::Imported);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn broken_archive_is_skipped() {
        let dir = temp("badzip");
        let archive = dir.join("broken.zip");
        fs::write(&archive, b"not a zip").unwrap();
        let mut mgr = PackageManager::new(dir.join("store")).unwrap();
        let report = add(&mut mgr, &archive, &import());
        assert!(matches!(report.skipped[0].reason, SkipReason::Unpack(_)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn linking_lists_the_folder_once_and_copies_nothing() {
        let dir = temp("link");
        let src = dir.join("BMS");
        song(&src.join("one"), "One");
        let library = dir.join("library.dat");
        let mut mgr = PackageManager::new(dir.join("store")).unwrap();
        let options = AddOptions {
            folders: FolderMode::Link(library.clone()),
            profile: PackProfile::Classic,
        };
        let report = add(&mut mgr, &src, &options);
        assert_eq!(report.linked.len(), 1);
        assert!(report.installed.is_empty());
        assert!(mgr.list_active_packages().is_empty());
        let listed = beetle_core::LibraryPaths::parse(&fs::read_to_string(&library).unwrap());
        assert_eq!(listed.paths(), report.linked.as_slice());

        let again = add(&mut mgr, &src, &options);
        assert!(again.linked.is_empty());
        assert!(matches!(again.skipped[0].reason, SkipReason::AlreadyLinked));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn cancelling_before_the_first_song_adds_nothing() {
        let dir = temp("cancel");
        let src = dir.join("src");
        song(&src.join("one"), "One");
        song(&src.join("two"), "Two");
        let mut mgr = PackageManager::new(dir.join("store")).unwrap();
        let cancel = AtomicBool::new(true);
        let result = add_path(&mut mgr, &src, &import(), Some(&cancel), |_| {});
        assert!(matches!(result, Err(AddError::Cancelled(r)) if r.added() == 0));
        assert!(mgr.list_active_packages().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_song_already_there_is_asked_about_and_skipped() {
        let dir = temp("judged");
        let have = dir.join("have");
        let again = dir.join("again");
        for d in [&have, &again] {
            fs::create_dir_all(d).unwrap();
            fs::write(
                d.join("song.bms"),
                "#TITLE Foo
#ARTIST Bar
#BPM 150
#WAV01 a.wav
#00111:01
",
            )
            .unwrap();
            fs::write(d.join("a.wav"), b"same bytes").unwrap();
        }
        let charts = Incoming::folder(&have).charts;
        let known = [KnownPlace {
            song: 0,
            at: crate::key_sets::PlaceAt::Folder(have.clone()),
            chart_file: "song.bms".into(),
            charts: charts.iter().map(|c| c.id).collect(),
            title: "Foo".into(),
            artist: "Bar".into(),
            label: "have".into(),
            package: None,
        }];
        let mut asked = Vec::new();
        let mut ask = |name: &str, v: &Verdict| {
            asked.push((name.to_string(), v.clone()));
            Answer::Skip
        };
        let judge = Judge {
            known: &known,
            ask: &mut ask,
        };
        let mut mgr = PackageManager::new(dir.join("store")).unwrap();
        let report = add_path_judged(&mut mgr, &again, &import(), judge, None, |_| {}).unwrap();
        assert_eq!(asked.len(), 1);
        assert!(matches!(asked[0].1, Verdict::AlreadyThere { .. }));
        assert_eq!(report.added(), 0);
        assert!(matches!(
            report.skipped[0].reason,
            SkipReason::AlreadyThere { .. }
        ));
        let _ = fs::remove_dir_all(dir);
    }
}
