//! A song inside a `.bmsp` package (`<package>::<entry>`).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use beetle_audio::SampleBank;
use beetle_core::{parse_bms, BmpId, BmsChart};
use beetle_render::{is_video_path, ImageBuffer};
use bms_package::atlas::BgaFrame;
use bms_package::PackageReader;

use super::{
    base_video_ids, progress, ChartLoadError, SongSource, StageArt, VideoSource,
    ARTWORK_CANDIDATE_FILENAMES, FALLBACK_VIDEO_NAMES,
};

pub(super) struct PackageSource {
    pkg_path: PathBuf,
    entry: String,
    /// The folder of `entry` inside the package, where its files are looked up.
    base_dir: String,
    pkg: PackageReader,
}

impl PackageSource {
    pub(super) fn open(pkg_path: &str, entry: &str) -> Result<Self, ChartLoadError> {
        let pkg = PackageReader::open_file(pkg_path).map_err(|_| {
            if Path::new(pkg_path).exists() {
                ChartLoadError::Unreadable
            } else {
                ChartLoadError::NotFound
            }
        })?;
        let base_dir = Path::new(entry)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_string_lossy()
            .into_owned();
        Ok(Self {
            pkg_path: PathBuf::from(pkg_path),
            entry: entry.to_string(),
            base_dir,
            pkg,
        })
    }

    /// An entry named by the manifest: its own path, or that name next to the chart.
    fn manifest_entry(&self, file: &str) -> Option<String> {
        if self.pkg.contains(file) {
            Some(file.to_string())
        } else {
            self.pkg.find_entry_path(&self.base_dir, file)
        }
    }

    fn read_near_chart(&mut self, name: &str) -> Option<Vec<u8>> {
        let path = self.pkg.find_entry_path(&self.base_dir, name)?;
        self.pkg.read_entry(&path).ok()
    }

    /// The chart's `#STAGEFILE` and `#BANNER` names, the non-empty ones.
    fn artwork_names(&mut self) -> Vec<String> {
        let Ok(bytes) = self.pkg.read_entry(&self.entry) else {
            return Vec::new();
        };
        let content = beetle_core::decode_bms_text(&bytes);
        let Ok(chart) = parse_bms(&content) else {
            return Vec::new();
        };
        [chart.header.stage_file, chart.header.banner]
            .into_iter()
            .filter(|n| !n.is_empty())
            .collect()
    }

    fn stage_from_delta(&mut self, names: &[String]) -> Option<StageArt> {
        let delta_meta = self.pkg.manifest().bga_delta.clone()?;
        let path = self.manifest_entry(&delta_meta.file)?;
        let delta_bytes = self.pkg.read_entry(&path).ok()?;
        let key = names.first().cloned().or_else(|| {
            ["stagefile", "stage", "banner", "title"]
                .into_iter()
                .find(|cand| {
                    delta_meta.frames.keys().any(|k| {
                        k.eq_ignore_ascii_case(cand)
                            || Path::new(k)
                                .file_stem()
                                .map(|s| s.eq_ignore_ascii_case(cand))
                                .unwrap_or(false)
                    })
                })
                .map(str::to_string)
        })?;
        let img_bytes = delta_meta.unpack_frame(&key, &delta_bytes).ok()??;
        let img = ImageBuffer::from_bytes(&img_bytes)?;
        Some(img)
    }

    fn stage_from_atlas(&mut self, names: &[String]) -> Option<StageArt> {
        let bga_meta = self.pkg.manifest().bga_atlas.clone()?;
        let path = self.manifest_entry(&bga_meta.file)?;
        let atlas_bytes = self.pkg.read_entry(&path).ok()?;
        let atlas_img = ImageBuffer::from_bytes(&atlas_bytes)?;
        let frame = bga_meta
            .frames
            .get("stagefile")
            .or_else(|| bga_meta.frames.get("banner"))
            .or_else(|| {
                names
                    .iter()
                    .find_map(|name| find_atlas_frame(&bga_meta.frames, name))
            })?;
        let img = atlas_img.crop(frame.x, frame.y, frame.width, frame.height)?;
        Some(img)
    }

    fn images_from_delta(
        &mut self,
        bmp_table: &HashMap<BmpId, String>,
        bga_bank: &mut HashMap<BmpId, ImageBuffer>,
    ) -> bool {
        let Some(delta_meta) = self.pkg.manifest().bga_delta.clone() else {
            return false;
        };
        let Some(path) = self.manifest_entry(&delta_meta.file) else {
            return false;
        };
        let Ok(delta_bytes) = self.pkg.read_entry(&path) else {
            return false;
        };
        let Ok(unpacked) = delta_meta.unpack_all(&delta_bytes) else {
            return false;
        };
        for (&bmp_id, filename) in bmp_table {
            if bga_bank.contains_key(&bmp_id) {
                continue;
            }
            let norm = filename.replace('\\', "/");
            let file_name = Path::new(&norm)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&norm);
            let norm_lower = norm.to_ascii_lowercase();
            let file_name_lower = file_name.to_ascii_lowercase();

            let found_bytes = unpacked
                .get(&norm)
                .or_else(|| unpacked.get(&norm_lower))
                .or_else(|| unpacked.get(file_name))
                .or_else(|| {
                    unpacked
                        .iter()
                        .find(|(k, _)| {
                            let k_norm = k.replace('\\', "/").to_ascii_lowercase();
                            let k_fn = Path::new(&k_norm)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or(&k_norm);
                            k_norm == norm_lower
                                || k_fn == file_name_lower
                                || k_norm.ends_with(&norm_lower)
                        })
                        .map(|(_, b)| b)
                });

            if let Some(img) = found_bytes.and_then(|b| ImageBuffer::from_bytes(b)) {
                bga_bank.insert(bmp_id, img);
            }
        }
        true
    }

    fn images_from_atlas(
        &mut self,
        bmp_table: &HashMap<BmpId, String>,
        bga_bank: &mut HashMap<BmpId, ImageBuffer>,
    ) -> bool {
        let Some(bga_meta) = self.pkg.manifest().bga_atlas.clone() else {
            return false;
        };
        let Some(path) = self.manifest_entry(&bga_meta.file) else {
            return false;
        };
        let Some(atlas_img) = self
            .pkg
            .read_entry(&path)
            .ok()
            .and_then(|b| ImageBuffer::from_bytes(&b))
        else {
            return false;
        };
        // Map chart BmpId -> chart filename -> atlas frame.
        for (&bmp_id, filename) in bmp_table {
            if bga_bank.contains_key(&bmp_id) {
                continue;
            }
            if let Some(frame) = find_atlas_frame(&bga_meta.frames, filename) {
                if let Some(sub_img) = atlas_img.crop(frame.x, frame.y, frame.width, frame.height) {
                    bga_bank.insert(bmp_id, sub_img);
                }
            }
        }
        true
    }

    /// Companion `.bga.bmsp` packages next to this one, most likely first.
    fn companion_candidates(&self) -> Vec<PathBuf> {
        let parent_dir = self.pkg_path.parent().unwrap_or_else(|| Path::new("."));
        let pkg_stem = self
            .pkg_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let manifest = self.pkg.manifest();

        let mut candidates = Vec::new();
        if let Some(ref companions) = manifest.companion_packages {
            if let Some(ref bga_info) = companions.bga {
                candidates.push(parent_dir.join(&bga_info.recommended_filename));
            }
        }
        if !pkg_stem.is_empty() {
            candidates.push(parent_dir.join(format!("{}.bga.bmsp", pkg_stem)));
        }
        candidates.push(parent_dir.join(format!("{}.bga.bmsp", manifest.id)));
        candidates.push(
            parent_dir
                .join(&manifest.id)
                .join("bga")
                .join(format!("{}.bga.bmsp", manifest.id)),
        );
        candidates
    }
}

impl SongSource for PackageSource {
    fn chart_bytes(&mut self) -> Result<Vec<u8>, ChartLoadError> {
        if !self.pkg.contains(&self.entry) {
            return Err(ChartLoadError::NotFound);
        }
        self.pkg
            .read_entry(&self.entry)
            .map_err(|_| ChartLoadError::Unreadable)
    }

    fn sounds(&mut self, chart: &BmsChart) -> (SampleBank, usize) {
        let mut soundbank = SampleBank::new();
        let mut loaded_count = 0;

        // Fast path: the package's Sound Atlas (1-pass zero-allocation decoding).
        if let Some(sound_meta) = self.pkg.manifest().sound_atlas.clone() {
            if let Some(path) = self.manifest_entry(&sound_meta.file) {
                if let Ok(atlas_bytes) = self.pkg.read_entry(&path) {
                    match SampleBank::load_from_sound_atlas_for_chart(
                        chart,
                        &sound_meta,
                        &atlas_bytes,
                    ) {
                        Ok(bank) => {
                            loaded_count = bank.len();
                            progress::set_done(loaded_count);
                            soundbank = bank;
                        }
                        Err(e) => {
                            eprintln!("[Loader] Sound Atlas decode error: {e}, falling back to individual file scan");
                        }
                    }
                }
            }
        }

        // Classic file-by-file loading for any keysound missing from the Sound Atlas.
        for (&wav_id, filename) in &chart.header.wav_table {
            if soundbank.contains_key(wav_id) {
                continue;
            }
            if let Some(bytes) = self.read_near_chart(filename) {
                if let Ok(pcm) = SampleBank::load_audio_from_bytes(&bytes) {
                    soundbank.insert(wav_id, pcm);
                    loaded_count += 1;
                    progress::set_done(loaded_count);
                }
            }
        }
        (soundbank, loaded_count)
    }

    fn images(&mut self, bmp_table: &HashMap<BmpId, String>) -> HashMap<BmpId, ImageBuffer> {
        // Fast paths: the BGA Delta Bundle, then the BGA Texture Atlas.
        progress::begin(progress::Stage::Images, 0);
        let mut bga_bank = HashMap::new();
        if self.images_from_delta(bmp_table, &mut bga_bank)
            || self.images_from_atlas(bmp_table, &mut bga_bank)
        {
            return bga_bank;
        }
        for (&bmp_id, filename) in bmp_table {
            if is_video_path(filename) {
                continue;
            }
            if let Some(img) = self
                .read_near_chart(filename)
                .and_then(|b| ImageBuffer::from_bytes(&b))
            {
                bga_bank.insert(bmp_id, img);
            }
        }
        bga_bank
    }

    fn videos(&mut self, chart: &BmsChart) -> HashMap<BmpId, VideoSource> {
        let mut video_sources = HashMap::new();
        let base_dir = self.base_dir.clone();
        load_videos_from_package_archive(&mut self.pkg, &base_dir, chart, &mut video_sources);
        if !video_sources.is_empty() {
            return video_sources;
        }
        for cand in self.companion_candidates() {
            if !cand.is_file() {
                continue;
            }
            if let Ok(mut bga_pkg) = PackageReader::open_file(&cand) {
                load_videos_from_package_archive(&mut bga_pkg, "", chart, &mut video_sources);
                if !video_sources.is_empty() {
                    println!(
                        "[Loader] Successfully loaded BGA companion package: '{}'",
                        cand.display()
                    );
                    break;
                }
            }
        }
        video_sources
    }

    fn stage_image(&mut self) -> Option<StageArt> {
        let names = self.artwork_names();
        if let Some(art) = self.stage_from_delta(&names) {
            return Some(art);
        }
        if let Some(art) = self.stage_from_atlas(&names) {
            return Some(art);
        }
        let candidates = names
            .iter()
            .map(String::as_str)
            .chain(ARTWORK_CANDIDATE_FILENAMES.iter().copied());
        for name in candidates.collect::<Vec<_>>() {
            if let Some(img_bytes) = self.read_near_chart(name) {
                if let Some(img) = ImageBuffer::from_bytes(&img_bytes) {
                    return Some(img);
                }
            }
        }
        None
    }
}

/// The atlas frame for a chart's image name: by its lower-cased path, or by
/// the original file name the frame was packed from.
fn find_atlas_frame<'a>(
    frames: &'a BTreeMap<String, BgaFrame>,
    name: &str,
) -> Option<&'a BgaFrame> {
    let norm = name.replace('\\', "/");
    let file_name = Path::new(&norm)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&norm);
    frames.get(&norm.to_ascii_lowercase()).or_else(|| {
        frames.values().find(|f| {
            f.original_filename
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case(&norm) || s.eq_ignore_ascii_case(file_name))
                .unwrap_or(false)
        })
    })
}

/// Finds the chart's videos inside a package: the BMP table's video files (or a
/// video beside an image of the same stem), else one of the usual movie names
/// played on the base layer.
pub(super) fn load_videos_from_package_archive(
    pkg: &mut PackageReader,
    base_dir: &str,
    chart: &BmsChart,
    video_sources: &mut HashMap<BmpId, VideoSource>,
) {
    let read = |pkg: &mut PackageReader, name: &str| -> Option<VideoSource> {
        let target_path = pkg.find_entry_path(base_dir, name)?;
        let bytes = pkg.read_entry(&target_path).ok()?;
        Some(VideoSource::Memory {
            bytes: Arc::from(bytes.into_boxed_slice()),
            filename_hint: Some(name.to_string()),
            display_aspect: None,
        })
    };

    for (&bmp_id, filename) in &chart.header.bmp_table {
        if is_video_path(filename) {
            if let Some(source) = read(pkg, filename) {
                video_sources.insert(bmp_id, source);
            }
        } else {
            let stem = match filename.rfind('.') {
                Some(pos) => &filename[..pos],
                None => filename.as_str(),
            };
            for ext in beetle_render::VIDEO_EXTENSIONS {
                if let Some(source) = read(pkg, &format!("{}.{}", stem, ext)) {
                    video_sources.insert(bmp_id, source);
                    break;
                }
            }
        }
    }

    // Fallback video inside package
    if !video_sources.is_empty() {
        return;
    }
    let header_videos = [&chart.header.stage_file, &chart.header.banner]
        .into_iter()
        .filter(|name| !name.is_empty() && is_video_path(name))
        .map(String::as_str);
    let fallback = header_videos
        .chain(FALLBACK_VIDEO_NAMES.iter().copied())
        .find(|name| pkg.find_entry_path(base_dir, name).is_some());
    if let Some(source) = fallback.and_then(|name| read(pkg, name)) {
        for id in base_video_ids(chart) {
            video_sources.entry(id).or_insert_with(|| source.clone());
        }
    }
}
