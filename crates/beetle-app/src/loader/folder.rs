//! A song that is a chart file in a folder, its files beside it.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use beetle_audio::SampleBank;
use beetle_core::{parse_bms, BmpId, BmsChart};
use beetle_render::{is_video_path, ImageBuffer};

use super::package::load_videos_from_package_archive;
use super::{
    base_video_ids, progress, ChartLoadError, SongSource, StageArt, VideoSource,
    ARTWORK_CANDIDATE_FILENAMES, FALLBACK_VIDEO_NAMES,
};

pub(super) struct FolderSource {
    chart: PathBuf,
    dir: PathBuf,
}

impl FolderSource {
    pub(super) fn new(chart: &str) -> Self {
        let chart = PathBuf::from(chart);
        let dir = chart
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Self { chart, dir }
    }
}

impl SongSource for FolderSource {
    fn chart_bytes(&mut self) -> Result<Vec<u8>, ChartLoadError> {
        fs::read(&self.chart).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ChartLoadError::NotFound,
            _ => ChartLoadError::Unreadable,
        })
    }

    fn sounds(&mut self, chart: &BmsChart) -> (SampleBank, usize) {
        SampleBank::load_chart_soundbank_with_progress(chart, &self.dir, &|done, _| {
            progress::set_done(done)
        })
    }

    fn images(&mut self, bmp_table: &HashMap<BmpId, String>) -> HashMap<BmpId, ImageBuffer> {
        progress::begin(progress::Stage::Images, bmp_table.len());
        load_images_parallel(&self.dir, bmp_table)
    }

    fn videos(&mut self, chart: &BmsChart) -> HashMap<BmpId, VideoSource> {
        let mut video_sources = find_video_files_in_dir(&self.dir, chart);
        if !video_sources.is_empty() {
            return video_sources;
        }
        // No videos in the folder: look for an adjacent companion package.
        let dir_name = self.dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let candidates = [
            self.dir.join(format!("{}.bga.bmsp", dir_name)),
            self.dir.join("bga.bmsp"),
        ];
        for cand in candidates {
            if !cand.is_file() {
                continue;
            }
            if let Ok(mut bga_pkg) = bms_package::PackageReader::open_file(&cand) {
                load_videos_from_package_archive(&mut bga_pkg, "", chart, &mut video_sources);
                if !video_sources.is_empty() {
                    println!(
                        "[Loader] Successfully loaded BGA companion package for folder: '{}'",
                        cand.display()
                    );
                    break;
                }
            }
        }
        video_sources
    }

    fn stage_image(&mut self) -> Option<StageArt> {
        // The chart header's stagefile or banner (not cached: the folder is as fast).
        if let Ok(bytes) = fs::read(&self.chart) {
            let content = beetle_core::decode_bms_text(&bytes);
            if let Ok(chart) = parse_bms(&content) {
                for name in [&chart.header.stage_file, &chart.header.banner] {
                    if name.is_empty() {
                        continue;
                    }
                    if let Some(img) = load_image_from_dir_or_case_insensitive(&self.dir, name) {
                        return Some(img);
                    }
                }
            }
        }

        // Fallback file scanning for common artwork names
        for name in ARTWORK_CANDIDATE_FILENAMES {
            let p = self.dir.join(name);
            if let Some(img) = ImageBuffer::load_from_file(&p) {
                return Some(img);
            }
        }
        None
    }
}

fn resolve_file_case_insensitive(dir: &Path, relative: &str) -> Option<PathBuf> {
    let normalized = relative.replace('\\', "/");
    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.is_empty() {
        return None;
    }

    let mut current = dir.to_path_buf();
    for part in parts {
        let mut found = false;
        if let Ok(entries) = fs::read_dir(&current) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.eq_ignore_ascii_case(part) {
                        current = entry.path();
                        found = true;
                        break;
                    }
                }
            }
        }
        if !found {
            return None;
        }
    }

    Some(current)
}

fn load_image_from_dir_or_case_insensitive(dir: &Path, filename: &str) -> Option<ImageBuffer> {
    let resolved = resolve_file_case_insensitive(dir, filename)?;
    ImageBuffer::load_from_file(&resolved)
}

fn find_video_files_in_dir(dir: &Path, chart: &BmsChart) -> HashMap<BmpId, VideoSource> {
    let mut videos = HashMap::new();

    // 1. Check bmp_table for video files or files with matching stems
    for (&bmp_id, filename) in &chart.header.bmp_table {
        if is_video_path(filename) {
            if let Some(p) = resolve_file_case_insensitive(dir, filename) {
                videos.insert(bmp_id, VideoSource::File(p));
            }
        } else {
            let stem = match filename.rfind('.') {
                Some(pos) => &filename[..pos],
                None => filename.as_str(),
            };
            for ext in beetle_render::VIDEO_EXTENSIONS {
                let candidate = format!("{}.{}", stem, ext);
                if let Some(p) = resolve_file_case_insensitive(dir, &candidate) {
                    videos.insert(bmp_id, VideoSource::File(p));
                    break;
                }
            }
        }
    }

    // 2. Fallback: if no bmp_table entry matched a video, check stage/banner or common names
    if videos.is_empty() {
        let header_video = [&chart.header.stage_file, &chart.header.banner]
            .into_iter()
            .filter(|name| !name.is_empty() && is_video_path(name))
            .find_map(|name| resolve_file_case_insensitive(dir, name));
        let fallback_path = header_video.or_else(|| {
            FALLBACK_VIDEO_NAMES
                .iter()
                .map(|name| dir.join(name))
                .find(|p| p.exists())
        });
        if let Some(fp) = fallback_path {
            let source = VideoSource::File(fp);
            for id in base_video_ids(chart) {
                videos.entry(id).or_insert_with(|| source.clone());
            }
        }
    }

    videos
}

/// Decodes the BGA images of a folder chart on several threads, counting them
/// in `progress` as they finish.
fn load_images_parallel(
    dir: &Path,
    bmp_table: &HashMap<BmpId, String>,
) -> HashMap<BmpId, ImageBuffer> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let jobs: Vec<(&BmpId, &String)> = bmp_table.iter().collect();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let out = Mutex::new(HashMap::new());
    let workers = thread::available_parallelism()
        .map_or(1, |n| n.get())
        .clamp(1, 8)
        .min(jobs.len().max(1));
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(&(&id, filename)) = jobs.get(i) else {
                    break;
                };
                if let Some(img) = load_image_from_dir_or_case_insensitive(dir, filename) {
                    if let Ok(mut map) = out.lock() {
                        map.insert(id, img);
                    }
                }
                progress::set_done(done.fetch_add(1, Ordering::Relaxed) + 1);
            });
        }
    });
    out.into_inner().unwrap_or_default()
}
