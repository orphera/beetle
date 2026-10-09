use crate::error::PackageManagerError;
use beetle_audio::{SampleBank, SoundAtlasBuilder};
use beetle_render::{is_video_path, BgaAtlasBuilder, ImageBuffer};
use bms_package::{
    split_sequence_prefix_and_num, BgaDeltaBuilder, Manifest, PackageBuilder, SoundAtlasCodec,
    MANIFEST_FILENAME,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Packaging profile selecting between legacy file-preserved ZIP and Dual Atlas instant-load package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PackProfile {
    /// Classic package: All individual wav, ogg, bmp files preserved inside ZIP archive.
    #[default]
    Classic,
    /// Turbo package: Audio and image files compiled into Sound Atlas and BGA Texture Atlas for instant loading.
    Turbo,
}

/// Mode controlling how high-entropy BGA video files are handled during packaging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BgaPackMode {
    /// Embed: Default legacy behavior. Video files are packaged directly inside the base .bmsp.
    #[default]
    Embed,
    /// Split: Videos are decoupled into a separate companion package (<id>.bga.bmsp),
    /// and companion metadata is attached to the base package manifest.
    Split,
    /// NoVideo: High-entropy video files are completely omitted from the package for minimal footprint.
    NoVideo,
}

/// Mode controlling how audio files are packaged and compressed in Sound Atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioPackMode {
    /// Auto: Preserves existing format (OggBundle for OGG songs, WavBundle for WAV songs, FlacBundle for FLAC songs).
    #[default]
    Auto,
    /// Flac: Losslessly compresses all WAV audio files into FLAC streams and packages as FlacBundle.
    Flac,
    /// Wav: Packs WAV files into WavBundle.
    Wav,
}

/// Comprehensive options controlling package generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PackOptions {
    pub profile: PackProfile,
    pub bga_mode: BgaPackMode,
    pub audio_mode: AudioPackMode,
}

impl PackOptions {
    pub fn new(profile: PackProfile, bga_mode: BgaPackMode) -> Self {
        Self {
            profile,
            bga_mode,
            audio_mode: AudioPackMode::Auto,
        }
    }

    pub fn with_audio_mode(mut self, audio_mode: AudioPackMode) -> Self {
        self.audio_mode = audio_mode;
        self
    }

    pub fn classic(bga_mode: BgaPackMode) -> Self {
        Self {
            profile: PackProfile::Classic,
            bga_mode,
            audio_mode: AudioPackMode::Auto,
        }
    }

    pub fn turbo(bga_mode: BgaPackMode) -> Self {
        Self {
            profile: PackProfile::Turbo,
            bga_mode,
            audio_mode: AudioPackMode::Auto,
        }
    }
}

/// Result of a packaging operation, supporting both single packages and decoupled base+companion pairs.
#[derive(Debug, Clone)]
pub struct PackOutput {
    /// The primary (base) package bytes (.bmsp).
    pub base_package: Vec<u8>,
    /// The BGA companion package bytes (.bga.bmsp), if split and videos exist.
    pub bga_package: Option<Vec<u8>>,
    /// Manifest of the base package.
    pub base_manifest: Manifest,
    /// Manifest of the BGA companion package, if generated.
    pub bga_manifest: Option<Manifest>,
}

/// Finds all BMS song root directories under a given directory.
///
/// A directory is considered a song root if it directly contains at least one BMS chart file
/// (`.bms`, `.bme`, `.bml`, `.pms`).
///
/// If `root` itself directly contains BMS files, `[root]` is returned immediately.
/// Otherwise, subdirectories are searched recursively using breadth-first traversal (BFS),
/// returning the shallowest subdirectories containing BMS files.
pub fn find_bms_song_roots<P: AsRef<Path>>(root: P) -> Vec<PathBuf> {
    let root = root.as_ref();
    if !root.is_dir() {
        return Vec::new();
    }

    fn has_bms_files(dir: &Path) -> bool {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        let ext_lower = ext.to_ascii_lowercase();
                        if matches!(ext_lower.as_str(), "bms" | "bme" | "bml" | "pms") {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    if has_bms_files(root) {
        return vec![root.to_path_buf()];
    }

    let mut song_roots = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root.to_path_buf());

    while let Some(current_dir) = queue.pop_front() {
        if let Ok(entries) = fs::read_dir(&current_dir) {
            let mut subdirs = Vec::new();
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !name.starts_with('.') {
                        subdirs.push(p);
                    }
                }
            }
            subdirs.sort();

            for subdir in subdirs {
                if has_bms_files(&subdir) {
                    song_roots.push(subdir);
                } else {
                    queue.push_back(subdir);
                }
            }
        }
    }

    song_roots
}

/// Analyzes a directory containing BMS files to automatically generate a `Manifest`.
pub fn analyze_bms_folder<P: AsRef<Path>>(dir_path: P) -> Result<Manifest, PackageManagerError> {
    let p = dir_path.as_ref();
    if !p.is_dir() {
        return Err(PackageManagerError::InvalidPackage(format!(
            "'{}' is not a directory",
            p.display()
        )));
    }

    let roots = find_bms_song_roots(p);
    let target_dir = match roots.as_slice() {
        [] => p,
        [single] => single.as_path(),
        [first, ..] => first.as_path(),
    };

    let dir_name = target_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("bms_song");

    let mut found_genre = String::new();

    // Look for .bms, .bme, .bml, .pms files in target_dir
    let mut bms_paths = Vec::new();
    for entry in fs::read_dir(target_dir)? {
        let path = entry?.path();
        if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            if matches!(ext.as_str(), "bms" | "bme" | "bml" | "pms") {
                bms_paths.push(path);
            }
        }
    }
    // read_dir order is platform-defined; sort so the chosen values are the same on every run.
    bms_paths.sort();

    let mut titles = Vec::new();
    let mut artists = Vec::new();
    for path in &bms_paths {
        if let Ok(bytes) = fs::read(path) {
            // Same decoding as the chart loader: a lossy UTF-8 read turned
            // Shift-JIS and CP949 titles into U+FFFD boxes in the registry.
            let content = beetle_core::decode_bms_text(&bytes);
            let (title, artist, genre) = extract_bms_header_tags(&content);
            if !title.is_empty() {
                titles.push(canonicalize_title(&title));
            }
            if !artist.is_empty() {
                artists.push(artist);
            }
            if !genre.is_empty() && found_genre.is_empty() {
                found_genre = genre;
            }
        }
    }

    let final_title = pick_most_common(&titles).unwrap_or_else(|| canonicalize_title(dir_name));

    let final_artist = pick_most_common(&artists).unwrap_or_else(|| "Unknown".to_string());

    let package_id = generate_slug_id(&final_artist, &final_title);

    let mut manifest = Manifest::new(package_id, final_title).with_author(final_artist);

    if !found_genre.is_empty() {
        manifest = manifest.with_extra("genre", serde_json::json!(found_genre));
    }

    Ok(manifest)
}

/// Packs an existing BMS directory into a standardized, deterministic `.bmsp` byte buffer (Classic profile).
pub fn pack_bms_folder<P: AsRef<Path>>(
    folder_path: P,
    manifest_override: Option<Manifest>,
) -> Result<Vec<u8>, PackageManagerError> {
    pack_bms_folder_profile(folder_path, manifest_override, PackProfile::Classic)
}

/// Packs an existing BMS directory using the specified packaging profile.
pub fn pack_bms_folder_profile<P: AsRef<Path>>(
    folder_path: P,
    manifest_override: Option<Manifest>,
    profile: PackProfile,
) -> Result<Vec<u8>, PackageManagerError> {
    pack_bms_folder_profile_with_progress(
        folder_path,
        manifest_override,
        profile,
        None,
        |_, _, _, _| {},
    )
}

/// Packs a BMS directory with cancellation check and progress reporting callback (Classic profile).
pub fn pack_bms_folder_with_progress<P: AsRef<Path>, F>(
    folder_path: P,
    manifest_override: Option<Manifest>,
    cancel_flag: Option<&std::sync::atomic::AtomicBool>,
    on_progress: F,
) -> Result<Vec<u8>, PackageManagerError>
where
    F: FnMut(&str, usize, usize, &str),
{
    pack_bms_folder_profile_with_progress(
        folder_path,
        manifest_override,
        PackProfile::Classic,
        cancel_flag,
        on_progress,
    )
}

/// Packs a BMS directory with profile, cancellation check, and progress reporting callback.
pub fn pack_bms_folder_profile_with_progress<P: AsRef<Path>, F>(
    folder_path: P,
    manifest_override: Option<Manifest>,
    profile: PackProfile,
    cancel_flag: Option<&std::sync::atomic::AtomicBool>,
    on_progress: F,
) -> Result<Vec<u8>, PackageManagerError>
where
    F: FnMut(&str, usize, usize, &str),
{
    let options = PackOptions::new(profile, BgaPackMode::Embed);
    let output = pack_bms_folder_advanced_with_progress(
        folder_path,
        manifest_override,
        options,
        cancel_flag,
        on_progress,
    )?;
    Ok(output.base_package)
}

/// Packs a BMS directory using advanced packaging options (profile and BGA mode).
pub fn pack_bms_folder_advanced_with_progress<P: AsRef<Path>, F>(
    folder_path: P,
    manifest_override: Option<Manifest>,
    options: PackOptions,
    cancel_flag: Option<&std::sync::atomic::AtomicBool>,
    mut on_progress: F,
) -> Result<PackOutput, PackageManagerError>
where
    F: FnMut(&str, usize, usize, &str),
{
    let roots = find_bms_song_roots(&folder_path);
    let target_dir = match roots.as_slice() {
        [] => {
            return Err(PackageManagerError::InvalidPackage(format!(
                "No BMS chart files (.bms, .bme, .bml, .pms) found in '{}' or any subdirectories",
                folder_path.as_ref().display()
            )));
        }
        [single] => single.as_path(),
        [first, ..] => first.as_path(),
    };

    match options.profile {
        PackProfile::Classic => {
            let p = target_dir;
            let mut manifest = match manifest_override {
                Some(m) => m,
                None => analyze_bms_folder(p)?,
            };

            let mut files_to_read = Vec::new();
            collect_file_paths(p, p, &mut files_to_read)?;

            let total = files_to_read.len();
            let mut base_files = Vec::new();
            let mut bga_files = Vec::new();

            for (i, (rel_path, abs_path)) in files_to_read.into_iter().enumerate() {
                if let Some(flag) = cancel_flag {
                    if flag.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err(PackageManagerError::Cancelled);
                    }
                }
                on_progress("Reading files", i + 1, total, &rel_path);
                let data = fs::read(&abs_path)?;

                if is_video_path(&rel_path) {
                    match options.bga_mode {
                        BgaPackMode::NoVideo => continue,
                        BgaPackMode::Split => {
                            bga_files.push((rel_path, data));
                            continue;
                        }
                        BgaPackMode::Embed => {
                            base_files.push((rel_path, data));
                        }
                    }
                } else {
                    base_files.push((rel_path, data));
                }
            }

            let mut bga_package_bytes = None;
            let mut bga_manifest_opt = None;

            if options.bga_mode == BgaPackMode::Split && !bga_files.is_empty() {
                let bga_id = format!("{}_bga", manifest.id);
                let bga_name = format!("{} (BGA Companion)", manifest.name);
                let mut bga_manifest =
                    Manifest::new_bga_companion(&bga_id, &bga_name, &manifest.id);
                bga_manifest.author = manifest.author.clone();

                let mut bga_builder = PackageBuilder::new(bga_manifest.clone());
                for (rel_path, data) in bga_files {
                    bga_builder.add_file(rel_path, data)?;
                }

                let bga_bytes =
                    bga_builder.build_to_bytes_with_progress(cancel_flag, |curr, tot, name| {
                        on_progress("Compressing .bga.bmsp", curr, tot, name);
                    })?;

                let bga_sha = bms_package::sha256_hex(&bga_bytes);
                let bga_size = bga_bytes.len() as u64;
                let rec_filename = format!("{}.bga.bmsp", manifest.id);
                let companion_info =
                    bms_package::BgaCompanionInfo::new(rec_filename, bga_size, bga_sha);
                manifest = manifest.with_bga_companion(companion_info);

                bga_package_bytes = Some(bga_bytes);
                bga_manifest_opt = Some(bga_manifest);
            }

            let mut builder = PackageBuilder::new(manifest.clone());
            for (rel_path, data) in base_files {
                builder.add_file(rel_path, data)?;
            }

            let base_bytes =
                builder.build_to_bytes_with_progress(cancel_flag, |curr, tot, name| {
                    on_progress("Compressing .bmsp", curr, tot, name);
                })?;

            Ok(PackOutput {
                base_package: base_bytes,
                bga_package: bga_package_bytes,
                base_manifest: manifest,
                bga_manifest: bga_manifest_opt,
            })
        }
        PackProfile::Turbo => pack_bms_folder_turbo_with_progress(
            target_dir,
            manifest_override,
            options,
            cancel_flag,
            on_progress,
        ),
    }
}

/// Turbo compilation: Assembles Sound Atlas and BGA Texture Atlas and builds a v2.0 package.
fn pack_bms_folder_turbo_with_progress<P: AsRef<Path>, F>(
    folder_path: P,
    manifest_override: Option<Manifest>,
    options: PackOptions,
    cancel_flag: Option<&std::sync::atomic::AtomicBool>,
    mut on_progress: F,
) -> Result<PackOutput, PackageManagerError>
where
    F: FnMut(&str, usize, usize, &str),
{
    let roots = find_bms_song_roots(&folder_path);
    let target_dir = match roots.as_slice() {
        [] => folder_path.as_ref(),
        [single] => single.as_path(),
        [first, ..] => first.as_path(),
    };

    let p = target_dir;
    let bga_mode = options.bga_mode;
    let mut manifest = match manifest_override {
        Some(m) => m,
        None => analyze_bms_folder(p)?,
    };

    let mut files_to_read = Vec::new();
    collect_file_paths(p, p, &mut files_to_read)?;

    // 1. Scan charts to map WavId and BmpId references to filenames
    // These maps are only used to discover referenced files that have an
    // unusual extension. Atlas entries themselves are keyed by norm_rel, not
    // by BMS IDs.
    let mut wav_targets: HashMap<String, ()> = HashMap::new();
    let mut bmp_targets: HashMap<String, ()> = HashMap::new();

    for (rel_path, abs_path) in &files_to_read {
        let ext = Path::new(rel_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if matches!(ext.as_str(), "bms" | "bme" | "bml" | "pms") {
            if let Ok(bytes) = fs::read(abs_path) {
                let content = beetle_core::decode_bms_text(&bytes);
                if let Ok(chart) = beetle_core::parse_bms(&content) {
                    for (&_wav_id, filename) in &chart.header.wav_table {
                        let norm = filename.replace('\\', "/").to_ascii_lowercase();
                        let file_only = Path::new(&norm)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&norm)
                            .to_string();
                        wav_targets.insert(norm.clone(), ());
                        wav_targets.insert(file_only.clone(), ());

                        // Cross-extension matching (.wav <-> .ogg <-> .flac)
                        let base_opt = norm
                            .strip_suffix(".wav")
                            .or_else(|| norm.strip_suffix(".ogg"))
                            .or_else(|| norm.strip_suffix(".flac"));
                        if let Some(base) = base_opt {
                            wav_targets.insert(format!("{}.wav", base), ());
                            wav_targets.insert(format!("{}.ogg", base), ());
                            wav_targets.insert(format!("{}.flac", base), ());
                        }
                        let fbase_opt = file_only
                            .strip_suffix(".wav")
                            .or_else(|| file_only.strip_suffix(".ogg"))
                            .or_else(|| file_only.strip_suffix(".flac"));
                        if let Some(fbase) = fbase_opt {
                            wav_targets.insert(format!("{}.wav", fbase), ());
                            wav_targets.insert(format!("{}.ogg", fbase), ());
                            wav_targets.insert(format!("{}.flac", fbase), ());
                        }
                    }
                    for (&_bmp_id, filename) in &chart.header.bmp_table {
                        let norm = filename.replace('\\', "/").to_ascii_lowercase();
                        let file_only = Path::new(&norm)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&norm)
                            .to_string();
                        let stem = Path::new(&file_only)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(&file_only);
                        bmp_targets.insert(norm.clone(), ());
                        bmp_targets.insert(file_only.clone(), ());

                        for alt_ext in &["bmp", "png", "jpg", "jpeg"] {
                            bmp_targets.insert(format!("{}.{}", stem, alt_ext), ());
                        }
                    }
                    if !chart.header.stage_file.is_empty() {
                        let norm = chart
                            .header
                            .stage_file
                            .replace('\\', "/")
                            .to_ascii_lowercase();
                        let file_only = Path::new(&norm)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&norm)
                            .to_string();
                        let stem = Path::new(&file_only)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(&file_only)
                            .to_string();
                        bmp_targets.insert(norm, ());
                        bmp_targets.insert(file_only, ());
                        for alt_ext in &["bmp", "png", "jpg", "jpeg"] {
                            bmp_targets.insert(format!("{}.{}", stem, alt_ext), ());
                        }
                    }
                    if !chart.header.banner.is_empty() {
                        let norm = chart.header.banner.replace('\\', "/").to_ascii_lowercase();
                        let file_only = Path::new(&norm)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&norm)
                            .to_string();
                        let stem = Path::new(&file_only)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(&file_only)
                            .to_string();
                        bmp_targets.insert(norm, ());
                        bmp_targets.insert(file_only, ());
                        for alt_ext in &["bmp", "png", "jpg", "jpeg"] {
                            bmp_targets.insert(format!("{}.{}", stem, alt_ext), ());
                        }
                    }
                }
            }
        }
    }

    let ogg_count = files_to_read
        .iter()
        .filter(|(rel, _)| rel.to_ascii_lowercase().ends_with(".ogg"))
        .count();
    let flac_count = files_to_read
        .iter()
        .filter(|(rel, _)| rel.to_ascii_lowercase().ends_with(".flac"))
        .count();
    let wav_count = files_to_read
        .iter()
        .filter(|(rel, _)| rel.to_ascii_lowercase().ends_with(".wav"))
        .count();

    let sound_codec = match options.audio_mode {
        AudioPackMode::Flac => SoundAtlasCodec::FlacBundle,
        AudioPackMode::Wav => SoundAtlasCodec::WavBundle,
        AudioPackMode::Auto => {
            if flac_count > 0 && flac_count >= wav_count && flac_count >= ogg_count {
                SoundAtlasCodec::FlacBundle
            } else if ogg_count > 0 && ogg_count >= wav_count {
                SoundAtlasCodec::OggBundle
            } else if wav_count > 0 {
                SoundAtlasCodec::WavBundle
            } else {
                SoundAtlasCodec::Pcm16
            }
        }
    };

    let mut image_prefix_counts: HashMap<String, usize> = HashMap::new();
    let mut total_image_count = 0;
    for (rel, _) in &files_to_read {
        let ext = Path::new(rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "bmp" | "png" | "jpg" | "jpeg") {
            total_image_count += 1;
            let (prefix, num_opt) = split_sequence_prefix_and_num(rel);
            if num_opt.is_some() {
                *image_prefix_counts.entry(prefix).or_default() += 1;
            }
        }
    }
    let use_bga_delta = image_prefix_counts.values().any(|&c| c >= 2) || total_image_count >= 8;

    let is_bundle = sound_codec.is_bundle();
    let mut sound_builder = SoundAtlasBuilder::new(sound_codec).with_padding_frames(128);
    let mut bga_builder = BgaAtlasBuilder::new(1);
    let mut bga_delta_builder = BgaDeltaBuilder::new();
    let mut passthrough_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut bga_files: Vec<(String, Vec<u8>)> = Vec::new();
    let total = files_to_read.len();
    for (i, (rel_path, abs_path)) in files_to_read.into_iter().enumerate() {
        if let Some(flag) = cancel_flag {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(PackageManagerError::Cancelled);
            }
        }

        on_progress("Compiling assets into Atlases", i + 1, total, &rel_path);

        let data = fs::read(&abs_path)?;
        let norm_rel = rel_path.replace('\\', "/").to_ascii_lowercase();
        let file_name = Path::new(&rel_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&rel_path);
        let ext = Path::new(&rel_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let norm_name = file_name.to_ascii_lowercase();
        // A. Video files
        if is_video_path(&rel_path) {
            match bga_mode {
                BgaPackMode::NoVideo => continue,
                BgaPackMode::Split => {
                    bga_files.push((rel_path, data));
                    continue;
                }
                BgaPackMode::Embed => {
                    passthrough_files.push((rel_path, data));
                    continue;
                }
            }
        }

        // B. Chart files -> Passthrough directly
        if matches!(ext.as_str(), "bms" | "bme" | "bml" | "pms") {
            passthrough_files.push((rel_path, data));
            continue;
        }

        // C. Audio files -> Compile into Sound Atlas
        let matched_wav_key = wav_targets
            .get(&norm_rel)
            .or_else(|| wav_targets.get(&norm_name))
            .is_some();
        let is_audio = matches!(ext.as_str(), "wav" | "ogg" | "flac") || matched_wav_key;
        if is_audio {
            if is_bundle {
                let final_data = if sound_codec == SoundAtlasCodec::FlacBundle && ext == "wav" {
                    match crate::flac::encode_wav_to_flac(&data) {
                        Ok(flac_data) => flac_data,
                        Err(_) => data,
                    }
                } else {
                    data
                };
                sound_builder.add_raw(norm_rel.clone(), final_data, Some(norm_rel.clone()));
                continue;
            } else if let Ok(pcm) = SampleBank::load_audio_from_bytes(&data) {
                sound_builder.add_sample(norm_rel.clone(), &pcm, Some(norm_rel.clone()));
                continue;
            }
        }

        // D. Image files -> Compile into BGA Delta Bundle or Texture Atlas
        let matched_bmp_key = bmp_targets
            .get(&norm_rel)
            .or_else(|| bmp_targets.get(&norm_name))
            .is_some();
        let is_image = matches!(ext.as_str(), "bmp" | "png" | "jpg" | "jpeg") || matched_bmp_key;
        if is_image {
            if use_bga_delta {
                bga_delta_builder.add_frame(norm_rel.clone(), data, Some(norm_rel.clone()));
                continue;
            } else if let Some(img) = ImageBuffer::from_bytes(&data) {
                bga_builder.add_frame(norm_rel.clone(), &img, Some(norm_rel.clone()));
                continue;
            }
        }

        // E. Other files (e.g. txt, readme, json) -> Passthrough
        passthrough_files.push((rel_path, data));
    }

    // 2. Build BGA Companion Package if split mode and videos exist
    let mut bga_package_bytes = None;
    let mut bga_manifest_opt = None;

    if bga_mode == BgaPackMode::Split && !bga_files.is_empty() {
        let bga_id = format!("{}_bga", manifest.id);
        let bga_name = format!("{} (BGA Companion)", manifest.name);
        let mut bga_manifest = Manifest::new_bga_companion(&bga_id, &bga_name, &manifest.id);
        bga_manifest.author = manifest.author.clone();

        let mut bga_builder = PackageBuilder::new(bga_manifest.clone());
        for (rel_path, data) in bga_files {
            bga_builder.add_file(rel_path, data)?;
        }

        let bga_bytes =
            bga_builder.build_to_bytes_with_progress(cancel_flag, |curr, tot, name| {
                on_progress("Compressing .bga.bmsp (Turbo)", curr, tot, name);
            })?;

        let bga_sha = bms_package::sha256_hex(&bga_bytes);
        let bga_size = bga_bytes.len() as u64;
        let rec_filename = format!("{}.bga.bmsp", manifest.id);
        let companion_info = bms_package::BgaCompanionInfo::new(rec_filename, bga_size, bga_sha);
        manifest = manifest.with_bga_companion(companion_info);

        bga_package_bytes = Some(bga_bytes);
        bga_manifest_opt = Some(bga_manifest);
    }

    // 3. Build Sound Atlas
    let (sound_meta, sound_bytes) = sound_builder.build("audio/atlas.bin").map_err(|e| {
        PackageManagerError::InvalidPackage(format!("Sound Atlas build error: {e}"))
    })?;

    manifest = manifest.with_sound_atlas(sound_meta);

    // 4. Build BGA Assets (BGA Delta Sequence Bundle or BGA Texture Atlas)
    if use_bga_delta && !bga_delta_builder.is_empty() {
        let (bga_delta_meta, bga_delta_bytes) = bga_delta_builder.build("visual/bga_delta.bin");
        manifest = manifest.with_bga_delta(bga_delta_meta);
        passthrough_files.push(("visual/bga_delta.bin".to_string(), bga_delta_bytes));
    } else if let Some((bga_meta, bga_image)) = bga_builder.build("visual/atlas.bmp") {
        let bga_bytes = bga_image.encode_bmp_bytes();
        manifest = manifest.with_bga_atlas(bga_meta);
        passthrough_files.push(("visual/atlas.bmp".to_string(), bga_bytes));
    }

    passthrough_files.push(("audio/atlas.bin".to_string(), sound_bytes));

    // 5. Assemble package archive with v2 Manifest
    let mut builder = PackageBuilder::new(manifest.clone());
    for (rel_path, data) in passthrough_files {
        builder.add_file(rel_path, data)?;
    }

    let base_bytes = builder.build_to_bytes_with_progress(cancel_flag, |curr, tot, name| {
        on_progress("Compressing .bmsp (Turbo)", curr, tot, name);
    })?;

    Ok(PackOutput {
        base_package: base_bytes,
        bga_package: bga_package_bytes,
        base_manifest: manifest,
        bga_manifest: bga_manifest_opt,
    })
}

fn collect_file_paths(
    base_dir: &Path,
    current_dir: &Path,
    out: &mut Vec<(String, std::path::PathBuf)>,
) -> Result<(), PackageManagerError> {
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_file_paths(base_dir, &path, out)?;
        } else if path.is_file() {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file_name == MANIFEST_FILENAME
                || file_name.ends_with(".bmsp")
                || file_name.starts_with('.')
            {
                continue;
            }

            if let Ok(rel) = path.strip_prefix(base_dir) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                out.push((rel_str, path));
            }
        }
    }
    Ok(())
}

fn extract_bms_header_tags(content: &str) -> (String, String, String) {
    let mut title = String::new();
    let mut artist = String::new();
    let mut genre = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') {
            continue;
        }

        let cmd_line = &trimmed[1..];
        let mut parts = cmd_line.splitn(2, |c: char| c.is_whitespace() || c == ':');
        let tag = parts.next().unwrap_or("").trim();
        let val = parts.next().unwrap_or("").trim();

        if tag.eq_ignore_ascii_case("TITLE") && title.is_empty() {
            title = val.to_string();
        } else if tag.eq_ignore_ascii_case("ARTIST") && artist.is_empty() {
            artist = val.to_string();
        } else if tag.eq_ignore_ascii_case("GENRE") && genre.is_empty() {
            genre = val.to_string();
        }
    }

    (title, artist, genre)
}

/// Strips chart-variant brackets such as `(14K Normal)` or `[Tora no another]` from a title.
///
/// A top-level bracket group is dropped when its content contains a difficulty, mode or key-count
/// token. Other brackets (`(Remix)`, `(Official)`) are kept. If nothing usable remains, the
/// original title is returned unchanged.
pub(crate) fn canonicalize_title(raw: &str) -> String {
    let mut kept = String::with_capacity(raw.len());
    let mut open_at = None;
    let mut depth = 0usize;

    for (i, c) in raw.char_indices() {
        if matches!(c, '(' | '[' | '{') {
            if depth == 0 {
                open_at = Some(i);
            }
            depth += 1;
        } else if matches!(c, ')' | ']' | '}') && depth > 0 {
            depth -= 1;
            if depth == 0 {
                let start = open_at.take().unwrap_or(i);
                if !is_variant_marker(&raw[start + 1..i]) {
                    kept.push_str(&raw[start..=i]);
                }
            }
        } else if depth == 0 {
            kept.push(c);
        }
    }
    // An unclosed bracket is not a variant marker we can trust, so keep it verbatim.
    if let Some(start) = open_at {
        kept.push_str(&raw[start..]);
    }

    let canonical = kept
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['-', '_', '/', ':'])
        .trim_end()
        .to_string();

    if canonical.is_empty() {
        raw.trim().to_string()
    } else {
        canonical
    }
}

/// Picks the value shared by the most charts in a folder (title or artist). Ties go to the
/// lexicographically smallest value, so the result does not depend on file order.
fn pick_most_common(values: &[String]) -> Option<String> {
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for value in values {
        *counts.entry(value.as_str()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(value, _)| value.to_string())
}

/// Difficulty and mode words that appear in chart-variant brackets. Matched as whole words.
const VARIANT_TOKENS: &[&str] = &[
    "beginner",
    "normal",
    "hyper",
    "another",
    "insane",
    "leggendaria",
    "expert",
    "oni",
    "sp",
    "dp",
    "spa",
    "spb",
    "spn",
    "sph",
    "spl",
    "dpa",
    "dpn",
    "dph",
];

fn is_variant_marker(inner: &str) -> bool {
    inner
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .any(|t| VARIANT_TOKENS.contains(&t.as_str()) || is_key_count(&t) || is_level_token(&t))
}

/// Matches key-count tokens such as `7k`, `14k`, `7key`, `14keys`.
fn is_key_count(token: &str) -> bool {
    ["keys", "key", "k"].iter().any(|suffix| {
        token
            .strip_suffix(suffix)
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Matches level tokens such as `lv12`, `level12`.
fn is_level_token(token: &str) -> bool {
    token
        .strip_prefix("lv")
        .or_else(|| token.strip_prefix("level"))
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn generate_slug_id(artist: &str, title: &str) -> String {
    let clean_artist = slugify(artist);
    let clean_title = slugify(title);

    if clean_artist.is_empty() {
        clean_title
    } else {
        format!("{}.{}", clean_artist, clean_title)
    }
}

fn slugify(s: &str) -> String {
    let mut slug = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if (c == ' ' || c == '_' || c == '-') && !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let trimmed = slug.trim_matches('_');
    if trimmed.is_empty() {
        "song".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_and_id_generation() {
        assert_eq!(slugify("DJ MAX - Techno"), "dj_max_techno");
        assert_eq!(slugify("곡 제목 (2026)"), "2026");
        assert_eq!(generate_slug_id("Tatsh", "RED ZONE"), "tatsh.red_zone");
    }

    #[test]
    fn test_canonicalize_title_strips_chart_variant_brackets() {
        assert_eq!(canonicalize_title("Song (14K Normal)"), "Song");
        assert_eq!(
            canonicalize_title("Tora no Song [Tora no another]"),
            "Tora no Song"
        );
        assert_eq!(
            canonicalize_title("aliceblue (Radio Edit) (SP ANOTHER)"),
            "aliceblue (Radio Edit)"
        );
        assert_eq!(
            canonicalize_title("곡 제목 [7K Hyper] (Remix)"),
            "곡 제목 (Remix)"
        );
        assert_eq!(canonicalize_title("Song [Lv12] (14keys)"), "Song");
    }

    #[test]
    fn test_canonicalize_title_keeps_ordinary_titles() {
        assert_eq!(
            canonicalize_title("Conflict (Official)"),
            "Conflict (Official)"
        );
        assert_eq!(canonicalize_title("Anotherway"), "Anotherway");
        assert_eq!(canonicalize_title("Unclosed (14K"), "Unclosed (14K");
        // Nothing left after stripping falls back to the original title.
        assert_eq!(canonicalize_title("(14K Normal)"), "(14K Normal)");
    }

    #[test]
    fn test_pick_most_common_uses_majority_then_smallest() {
        let values = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            pick_most_common(&values(&["Song", "Other", "Song"])),
            Some("Song".to_string())
        );
        assert_eq!(
            pick_most_common(&values(&["Zeta", "Alpha"])),
            Some("Alpha".to_string())
        );
        assert_eq!(pick_most_common(&[]), None);
    }

    #[test]
    fn test_analyze_bms_folder_votes_over_all_charts() {
        let dir = std::env::temp_dir().join(format!(
            "bpm_analyze_vote_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.bms"), "#TITLE Song (Odd Remix)\n#ARTIST Odd\n").unwrap();
        fs::write(dir.join("b.bme"), "#TITLE Song (14K Normal)\n#ARTIST X\n").unwrap();
        fs::write(
            dir.join("c.pms"),
            "#TITLE Song [Tora no another]\n#ARTIST X\n",
        )
        .unwrap();
        fs::write(dir.join("d.bml"), "#TITLE Song (14K Hyper)\n#ARTIST Y\n").unwrap();

        let manifest = analyze_bms_folder(&dir).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(manifest.name, "Song");
        assert_eq!(manifest.author.as_deref(), Some("X"));
        assert_eq!(manifest.id, "x.song");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_analyze_bms_folder_keeps_legacy_encoded_title() {
        let dir = std::env::temp_dir().join(format!(
            "bpm_analyze_legacy_title_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        // "#TITLE 가나다" in CP949 (EUC-KR), which is not valid UTF-8.
        let mut bytes = b"#TITLE ".to_vec();
        bytes.extend_from_slice(&[0xB0, 0xA1, 0xB3, 0xAA, 0xB4, 0xD9]);
        bytes.extend_from_slice(b"\n#ARTIST X\n");
        fs::write(dir.join("a.bms"), bytes).unwrap();

        let manifest = analyze_bms_folder(&dir).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(manifest.name, "가나다");
        assert!(!manifest.name.contains('\u{FFFD}'));
    }

    #[test]
    fn test_extract_bms_header_tags() {
        let bms = r#"
#TITLE Happy Synthesizer
#ARTIST EasyPop
#GENRE Electro Pop
#BPM 128
#00111:01000000
"#;
        let (title, artist, genre) = extract_bms_header_tags(bms);
        assert_eq!(title, "Happy Synthesizer");
        assert_eq!(artist, "EasyPop");
        assert_eq!(genre, "Electro Pop");
    }

    #[test]
    fn test_pack_bms_folder_turbo_dual_atlas() {
        use beetle_render::skin::ColorRgba;
        use bms_package::Package;

        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_turbo_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Create a minimal chart referencing 01.wav and 01.bmp
        let bms_content = r#"
#TITLE Turbo Song
#ARTIST Beetle
#BPM 150
#WAV01 kick.wav
#WAV02 snare.wav
#BMP01 bg.bmp
#STAGEFILE stage.bmp
#00111:0102
"#;
        fs::write(temp_dir.join("main.bme"), bms_content).unwrap();

        // 2. Create minimal WAV files
        let create_wav = |samples: &[i16]| -> Vec<u8> {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut cur = std::io::Cursor::new(Vec::new());
            {
                let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
                for &s in samples {
                    w.write_sample(s).unwrap();
                }
                w.finalize().unwrap();
            }
            cur.into_inner()
        };

        fs::write(temp_dir.join("kick.wav"), create_wav(&[1000, 2000, -1000])).unwrap();
        fs::write(temp_dir.join("snare.wav"), create_wav(&[500, -500, 300])).unwrap();

        // 3. Create minimal BMP images
        let img1 = ImageBuffer::new(32, 32, ColorRgba::new(255, 0, 0, 255));
        let img2 = ImageBuffer::new(64, 48, ColorRgba::new(0, 255, 0, 255));
        fs::write(temp_dir.join("bg.bmp"), img1.encode_bmp_bytes()).unwrap();
        fs::write(temp_dir.join("stage.bmp"), img2.encode_bmp_bytes()).unwrap();

        // 4. Pack with Turbo profile
        let pkg_bytes = pack_bms_folder_profile(&temp_dir, None, PackProfile::Turbo)
            .expect("turbo pack failed");

        // 5. Inspect and verify package structure
        let pkg = Package::from_bytes(pkg_bytes).expect("package inspect failed");
        let manifest = pkg.manifest();

        assert_eq!(manifest.format, 2);
        assert!(manifest.sound_atlas.is_some());
        assert!(manifest.bga_atlas.is_some());

        let sound_atlas = manifest.sound_atlas.as_ref().unwrap();
        assert_eq!(sound_atlas.file, "audio/atlas.bin");
        // Atlas keys are normalized asset paths, not chart-local BMS IDs.
        assert!(sound_atlas.slices.contains_key("kick.wav"));
        assert!(sound_atlas.slices.contains_key("snare.wav"));

        let bga_atlas = manifest.bga_atlas.as_ref().unwrap();
        assert_eq!(bga_atlas.file, "visual/atlas.bmp");
        assert!(bga_atlas.frames.contains_key("bg.bmp"));
        assert!(bga_atlas.frames.contains_key("stage.bmp"));

        // Verify package entries
        assert!(pkg.contains("audio/atlas.bin"));
        assert!(pkg.contains("visual/atlas.bmp"));
        assert!(pkg.contains("main.bme"));
        // Individual audio and images should NOT be stored as standalone entries
        assert!(!pkg.contains("kick.wav"));
        assert!(!pkg.contains("snare.wav"));
        assert!(!pkg.contains("bg.bmp"));
        assert!(!pkg.contains("stage.bmp"));

        // 6. Test decoding sound atlas into SampleBank
        let atlas_bytes = pkg
            .read_entry("audio/atlas.bin")
            .expect("read atlas bin failed");
        let chart_bytes = pkg.read_entry("main.bme").expect("read chart failed");
        let chart_text = beetle_core::decode_bms_text(&chart_bytes);
        let chart = beetle_core::parse_bms(&chart_text).expect("parse chart failed");
        let sample_bank =
            SampleBank::load_from_sound_atlas_for_chart(&chart, sound_atlas, &atlas_bytes)
                .expect("load sound atlas failed");
        assert_eq!(sample_bank.len(), 2);

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_find_bms_song_roots_nested_and_batch() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_roots_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Direct song
        let song_a = temp_dir.join("SongA");
        fs::create_dir_all(&song_a).unwrap();
        fs::write(song_a.join("track.bms"), "#TITLE Track A").unwrap();

        let roots_a = find_bms_song_roots(&song_a);
        assert_eq!(roots_a.len(), 1);
        assert_eq!(roots_a[0], song_a);

        // 2. Nested wrapper song (wrapper/actual/track.bms)
        let wrapper = temp_dir.join("Wrapper");
        let nested_song = wrapper.join("ActualSong");
        fs::create_dir_all(&nested_song).unwrap();
        fs::write(nested_song.join("track.bme"), "#TITLE Nested").unwrap();

        let roots_wrapper = find_bms_song_roots(&wrapper);
        assert_eq!(roots_wrapper.len(), 1);
        assert_eq!(roots_wrapper[0], nested_song);

        // 3. Multi-song pack (temp_dir contains SongA and Wrapper/ActualSong)
        let all_roots = find_bms_song_roots(&temp_dir);
        assert_eq!(all_roots.len(), 2);
        assert!(all_roots.contains(&song_a));
        assert!(all_roots.contains(&nested_song));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_pack_cross_format_and_nested_subfolder_matching() {
        use beetle_render::skin::ColorRgba;
        use bms_package::Package;

        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_cross_fmt_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested_song = temp_dir.join("ArchiveFolder").join("SongRoot");
        fs::create_dir_all(&nested_song).unwrap();

        // Chart references .wav and .bmp
        let bms_content = r#"
#TITLE Cross Format Song
#ARTIST Specialist
#BPM 180
#WAV01 kick.wav
#WAV02 sound/snare.wav
#BMP01 bg.bmp
#STAGEFILE stage.bmp
#00111:0102
"#;
        fs::write(nested_song.join("play.bms"), bms_content).unwrap();

        // Disk has .wav and .bmp or .png
        let create_wav = |samples: &[i16]| -> Vec<u8> {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut cur = std::io::Cursor::new(Vec::new());
            {
                let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
                for &s in samples {
                    w.write_sample(s).unwrap();
                }
                w.finalize().unwrap();
            }
            cur.into_inner()
        };

        // kick.wav directly, and snare.wav in sound/ subfolder
        let snd_dir = nested_song.join("sound");
        fs::create_dir_all(&snd_dir).unwrap();
        fs::write(nested_song.join("kick.wav"), create_wav(&[100, 200])).unwrap();
        fs::write(snd_dir.join("snare.wav"), create_wav(&[300, -300])).unwrap();

        // bg.bmp and stage.bmp
        let img = ImageBuffer::new(16, 16, ColorRgba::new(255, 128, 0, 255)).encode_bmp_bytes();
        fs::write(nested_song.join("bg.bmp"), &img).unwrap();
        fs::write(nested_song.join("stage.bmp"), &img).unwrap();

        // Pack from the TOP outer wrapper folder (ArchiveFolder)
        let outer_dir = temp_dir.join("ArchiveFolder");
        let pkg_bytes = pack_bms_folder_profile(&outer_dir, None, PackProfile::Turbo)
            .expect("should automatically resolve nested song root and pack");

        let pkg = Package::from_bytes(pkg_bytes).expect("inspect package failed");
        let manifest = pkg.manifest();
        assert_eq!(manifest.name, "Cross Format Song");
        assert_eq!(manifest.author.as_deref(), Some("Specialist"));

        let sound_meta = manifest.sound_atlas.as_ref().unwrap();
        assert!(
            sound_meta.slices.contains_key("kick.wav"),
            "kick.wav matched"
        );
        assert!(
            sound_meta.slices.contains_key("sound/snare.wav"),
            "sound/snare.wav matched"
        );

        let bga_meta = manifest.bga_atlas.as_ref().unwrap();
        assert!(bga_meta.frames.contains_key("bg.bmp"), "bg.bmp matched");
        assert!(
            bga_meta.frames.contains_key("stage.bmp"),
            "stage.bmp matched"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_pack_bms_folder_turbo_ogg_bundle() {
        use bms_package::Package;

        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_ogg_bundle_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let song_dir = temp_dir.join("OggSong");
        fs::create_dir_all(&song_dir).unwrap();

        let bms_content = r#"
#TITLE Ogg Bundle Song
#ARTIST SoundMaster
#WAV01 01.ogg
#WAV02 02.ogg
#00111:0102
"#;
        fs::write(song_dir.join("play.bms"), bms_content).unwrap();
        let ogg1_bytes = b"OggS_test_payload_sample_01_sound_stream_data_here";
        let ogg2_bytes = b"OggS_test_payload_sample_02_sound_stream_data_here_longer";
        fs::write(song_dir.join("01.ogg"), ogg1_bytes).unwrap();
        fs::write(song_dir.join("02.ogg"), ogg2_bytes).unwrap();

        let pkg_bytes =
            pack_bms_folder_profile(&song_dir, None, PackProfile::Turbo).expect("pack failed");
        let pkg = Package::from_bytes(pkg_bytes).expect("package open failed");
        let manifest = pkg.manifest();

        assert_eq!(manifest.name, "Ogg Bundle Song");
        let sound_meta = manifest.sound_atlas.as_ref().expect("sound atlas missing");
        assert_eq!(sound_meta.codec, bms_package::SoundAtlasCodec::OggBundle);
        assert_eq!(sound_meta.slices.len(), 2);

        let slice1 = sound_meta.slices.get("01.ogg").unwrap();
        let slice2 = sound_meta.slices.get("02.ogg").unwrap();
        assert_eq!(slice1.byte_len(), ogg1_bytes.len() as u64);
        assert_eq!(slice2.byte_len(), ogg2_bytes.len() as u64);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_pack_bms_folder_turbo_wav_bundle() {
        use bms_package::Package;

        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_wav_bundle_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let song_dir = temp_dir.join("WavSong");
        fs::create_dir_all(&song_dir).unwrap();

        let bms_content = r#"
#TITLE Wav Bundle Song
#ARTIST WavMaster
#WAV01 01.wav
#WAV02 02.wav
#00111:0102
"#;
        fs::write(song_dir.join("play.bms"), bms_content).unwrap();
        let wav1_bytes = b"RIFF_test_payload_sample_01_wav_stream_data_here";
        let wav2_bytes = b"RIFF_test_payload_sample_02_wav_stream_data_here_longer";
        fs::write(song_dir.join("01.wav"), wav1_bytes).unwrap();
        fs::write(song_dir.join("02.wav"), wav2_bytes).unwrap();

        let pkg_bytes =
            pack_bms_folder_profile(&song_dir, None, PackProfile::Turbo).expect("pack failed");
        let pkg = Package::from_bytes(pkg_bytes).expect("package open failed");
        let manifest = pkg.manifest();

        assert_eq!(manifest.name, "Wav Bundle Song");
        let sound_meta = manifest.sound_atlas.as_ref().expect("sound atlas missing");
        assert_eq!(sound_meta.codec, bms_package::SoundAtlasCodec::WavBundle);
        assert_eq!(sound_meta.slices.len(), 2);

        let slice1 = sound_meta.slices.get("01.wav").unwrap();
        let slice2 = sound_meta.slices.get("02.wav").unwrap();
        assert_eq!(slice1.byte_len(), wav1_bytes.len() as u64);
        assert_eq!(slice2.byte_len(), wav2_bytes.len() as u64);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_pack_turbo_preserves_same_wav_id_across_charts() {
        use bms_package::Package;

        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_wav_id_collision_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let chart_7key = "#TITLE Collision\n#WAVC4 lovinit.wav\n#00111:C4\n";
        let chart_14key = "#TITLE Collision\n#WAVC4 lovinitl.wav\n#00111:C4\n";
        fs::write(temp_dir.join("7key.bms"), chart_7key).unwrap();
        fs::write(temp_dir.join("14key.bms"), chart_14key).unwrap();

        let make_wav = |sample: i16| {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut cursor = std::io::Cursor::new(Vec::new());
            let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
            writer.write_sample(sample).unwrap();
            writer.finalize().unwrap();
            cursor.into_inner()
        };
        fs::write(temp_dir.join("lovinit.wav"), make_wav(1000)).unwrap();
        fs::write(temp_dir.join("lovinitl.wav"), make_wav(-1000)).unwrap();

        let package_bytes = pack_bms_folder_profile(&temp_dir, None, PackProfile::Turbo)
            .expect("turbo pack failed");
        let package = Package::from_bytes(package_bytes).expect("package inspect failed");
        let meta = package.manifest().sound_atlas.as_ref().unwrap();

        assert_eq!(meta.slices.len(), 2);
        assert!(meta
            .slices
            .values()
            .any(|s| s.original_filename.as_deref() == Some("lovinit.wav")));
        assert!(meta
            .slices
            .values()
            .any(|s| s.original_filename.as_deref() == Some("lovinitl.wav")));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_pack_bga_split_and_no_video_modes() {
        use bms_package::{Package, PackageType};

        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_bga_split_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let song_dir = temp_dir.join("VideoSong");
        fs::create_dir_all(&song_dir).unwrap();

        let bms_content = r#"
#TITLE Video Song
#ARTIST BGA Master
#WAV01 01.wav
#BMP01 bga.mp4
#00111:01
#00104:01
"#;
        fs::write(song_dir.join("main.bms"), bms_content).unwrap();
        fs::write(song_dir.join("01.wav"), b"RIFF_sample_sound_data_wav").unwrap();
        fs::write(song_dir.join("bga.mp4"), vec![0x11u8; 4000]).unwrap(); // 4000 bytes video
        fs::write(song_dir.join("movie.mpg"), vec![0x55u8; 1000]).unwrap(); // 1000 bytes video

        // 1. Test BgaPackMode::Embed (Turbo)
        let embed_opts = PackOptions::turbo(BgaPackMode::Embed);
        let embed_out = pack_bms_folder_advanced_with_progress(
            &song_dir,
            None,
            embed_opts,
            None,
            |_, _, _, _| {},
        )
        .expect("embed pack failed");
        assert!(embed_out.bga_package.is_none());
        assert!(embed_out.bga_manifest.is_none());
        assert!(embed_out.base_manifest.companion_packages.is_none());

        let embed_pkg = Package::from_bytes(embed_out.base_package).unwrap();
        assert!(embed_pkg.contains("bga.mp4"));
        assert!(embed_pkg.contains("movie.mpg"));

        // 2. Test BgaPackMode::NoVideo (Turbo)
        let novideo_opts = PackOptions::turbo(BgaPackMode::NoVideo);
        let novideo_out = pack_bms_folder_advanced_with_progress(
            &song_dir,
            None,
            novideo_opts,
            None,
            |_, _, _, _| {},
        )
        .expect("novideo pack failed");
        assert!(novideo_out.bga_package.is_none());
        assert!(novideo_out.bga_manifest.is_none());
        assert!(novideo_out.base_manifest.companion_packages.is_none());

        let novideo_pkg = Package::from_bytes(novideo_out.base_package).unwrap();
        assert!(!novideo_pkg.contains("bga.mp4"));
        assert!(!novideo_pkg.contains("movie.mpg"));

        // 3. Test BgaPackMode::Split (Turbo)
        let split_opts = PackOptions::turbo(BgaPackMode::Split);
        let split_out = pack_bms_folder_advanced_with_progress(
            &song_dir,
            None,
            split_opts,
            None,
            |_, _, _, _| {},
        )
        .expect("split pack failed");
        assert!(split_out.bga_package.is_some());
        assert!(split_out.bga_manifest.is_some());

        // Base package checks
        let base_pkg = Package::from_bytes(split_out.base_package).unwrap();
        assert!(!base_pkg.contains("bga.mp4"));
        assert!(!base_pkg.contains("movie.mpg"));
        assert_eq!(base_pkg.manifest().package_type, PackageType::Standard);

        // Companion metadata attached to base package
        let bga_comp = base_pkg
            .manifest()
            .companion_packages
            .as_ref()
            .unwrap()
            .bga
            .as_ref()
            .unwrap();
        assert_eq!(
            bga_comp.recommended_filename,
            format!("{}.bga.bmsp", base_pkg.manifest().id)
        );

        // Companion package checks
        let bga_pkg_bytes = split_out.bga_package.unwrap();
        assert_eq!(bga_comp.size_bytes, bga_pkg_bytes.len() as u64);
        assert_eq!(bga_comp.sha256, bms_package::sha256_hex(&bga_pkg_bytes));

        let bga_pkg = Package::from_bytes(bga_pkg_bytes).unwrap();
        assert!(bga_pkg.contains("bga.mp4"));
        assert!(bga_pkg.contains("movie.mpg"));
        assert_eq!(bga_pkg.manifest().package_type, PackageType::BgaCompanion);
        assert_eq!(
            bga_pkg.manifest().target_package_id.as_deref(),
            Some(base_pkg.manifest().id.as_str())
        );

        // 4. Test BgaPackMode::Split (Classic)
        let classic_split_opts = PackOptions::classic(BgaPackMode::Split);
        let classic_out = pack_bms_folder_advanced_with_progress(
            &song_dir,
            None,
            classic_split_opts,
            None,
            |_, _, _, _| {},
        )
        .expect("classic split pack failed");
        assert!(classic_out.bga_package.is_some());
        let classic_base = Package::from_bytes(classic_out.base_package).unwrap();
        assert!(!classic_base.contains("bga.mp4"));
        assert!(classic_base.contains("01.wav"));

        let classic_bga = Package::from_bytes(classic_out.bga_package.unwrap()).unwrap();
        assert!(classic_bga.contains("bga.mp4"));
        assert_eq!(
            classic_bga.manifest().package_type,
            PackageType::BgaCompanion
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_pack_bms_folder_turbo_bga_delta_and_export_vfs() {
        use crate::export::export_package_to_folder;
        use crate::vfs::VirtualBmsFs;
        use beetle_render::skin::ColorRgba;
        use beetle_render::ImageBuffer;
        use bms_package::{BgaFrameType, Package};

        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_bga_delta_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let song_dir = temp_dir.join("DeltaSong");
        fs::create_dir_all(&song_dir).unwrap();

        // 1. Create a BMS chart with an animation loop (loop_01, loop_02, loop_03)
        let bms_content = r#"
#TITLE Delta Animation Song
#ARTIST Animator
#BPM 140
#WAV01 01.wav
#BMP01 loop_01.bmp
#BMP02 loop_02.bmp
#BMP03 loop_03.bmp
#STAGEFILE stage.bmp
#00111:01
"#;
        fs::write(song_dir.join("main.bms"), bms_content).unwrap();

        // 2. Audio file
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let wav_path = song_dir.join("01.wav");
        {
            let mut w = hound::WavWriter::create(&wav_path, spec).unwrap();
            for s in [1000i16, 2000, 3000, 0] {
                w.write_sample(s).unwrap();
            }
            w.finalize().unwrap();
        }

        // 3. Animation loop images (32x32): loop_01 (red), loop_02 (red with tiny diff), loop_03 (red with tiny diff)
        let img1 = ImageBuffer::new(32, 32, ColorRgba::new(200, 30, 30, 255));
        let raw1 = img1.encode_bmp_bytes();
        fs::write(song_dir.join("loop_01.bmp"), &raw1).unwrap();

        // loop_02 modifies a few pixels
        let mut img2 = img1.clone();
        img2.pixels[10 * 32 + 10] = ColorRgba::new(200, 200, 30, 255);
        img2.pixels[10 * 32 + 11] = ColorRgba::new(200, 200, 30, 255);
        let raw2 = img2.encode_bmp_bytes();
        fs::write(song_dir.join("loop_02.bmp"), &raw2).unwrap();

        // loop_03 modifies a few pixels
        let mut img3 = img2.clone();
        img3.pixels[15 * 32 + 15] = ColorRgba::new(30, 200, 200, 255);
        img3.pixels[15 * 32 + 16] = ColorRgba::new(30, 200, 200, 255);
        let raw3 = img3.encode_bmp_bytes();
        fs::write(song_dir.join("loop_03.bmp"), &raw3).unwrap();

        // Standalone stage image
        let stage_img = ImageBuffer::new(64, 48, ColorRgba::new(50, 100, 150, 255));
        let raw_stage = stage_img.encode_bmp_bytes();
        fs::write(song_dir.join("stage.bmp"), &raw_stage).unwrap();

        // 4. Pack into Turbo BMSP package
        let pkg_bytes = pack_bms_folder_profile(&song_dir, None, PackProfile::Turbo)
            .expect("turbo packing should succeed");
        let pkg_path = temp_dir.join("delta_song.bmsp");
        fs::write(&pkg_path, &pkg_bytes).unwrap();

        let pkg = Package::from_bytes(pkg_bytes).expect("open package failed");
        let manifest = pkg.manifest();

        // Verify BgaDelta was selected and populated
        let delta_meta = manifest
            .bga_delta
            .as_ref()
            .expect("bga_delta should be present");
        assert!(
            manifest.bga_atlas.is_none(),
            "2D atlas should not be built when bga_delta is used"
        );
        assert_eq!(delta_meta.total_frames, 4);

        // Verify loop_01 is Keyframe, loop_02 and loop_03 are Delta frames
        let f1 = delta_meta
            .frames
            .get("loop_01.bmp")
            .expect("loop_01 in frames");
        assert_eq!(f1.frame_type, BgaFrameType::Keyframe);
        assert!(f1.parent.is_none());

        let f2 = delta_meta
            .frames
            .get("loop_02.bmp")
            .expect("loop_02 in frames");
        assert_eq!(f2.frame_type, BgaFrameType::Delta);
        assert_eq!(f2.parent.as_deref(), Some("loop_01.bmp"));

        let f3 = delta_meta
            .frames
            .get("loop_03.bmp")
            .expect("loop_03 in frames");
        assert_eq!(f3.frame_type, BgaFrameType::Delta);
        assert_eq!(f3.parent.as_deref(), Some("loop_02.bmp"));

        // 5. Test Exporter: unpacks and restores exact original BMP bytes
        let export_dir = temp_dir.join("Exported");
        let stats =
            export_package_to_folder(&pkg_path, &export_dir).expect("export should succeed");
        assert_eq!(stats.bga_files, 4);

        let exported_raw1 =
            fs::read(export_dir.join("loop_01.bmp")).expect("read exported loop_01");
        assert_eq!(exported_raw1, raw1, "loop_01.bmp bit-exact roundtrip");

        let exported_raw2 =
            fs::read(export_dir.join("loop_02.bmp")).expect("read exported loop_02");
        assert_eq!(exported_raw2, raw2, "loop_02.bmp bit-exact roundtrip");

        let exported_raw3 =
            fs::read(export_dir.join("loop_03.bmp")).expect("read exported loop_03");
        assert_eq!(exported_raw3, raw3, "loop_03.bmp bit-exact roundtrip");

        let exported_stage = fs::read(export_dir.join("stage.bmp")).expect("read exported stage");
        assert_eq!(exported_stage, raw_stage, "stage.bmp bit-exact roundtrip");

        // 6. Test VFS: mounts BGA Delta virtual files seamlessly
        let mut vfs = VirtualBmsFs::new();
        vfs.mount_package("delta_song", &pkg_path)
            .expect("mount should succeed");

        assert_eq!(vfs.read_file("delta_song/loop_01.bmp").unwrap(), raw1);
        assert_eq!(vfs.read_file("delta_song/loop_02.bmp").unwrap(), raw2);
        assert_eq!(vfs.read_file("delta_song/loop_03.bmp").unwrap(), raw3);
        assert_eq!(vfs.read_file("delta_song/stage.bmp").unwrap(), raw_stage);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_pack_with_flac_bundle_roundtrip() {
        use crate::export::export_package_to_folder;
        use crate::vfs::VirtualBmsFs;
        use bms_package::Package;

        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_flac_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let song_dir = temp_dir.join("Song");
        fs::create_dir_all(&song_dir).unwrap();

        // 1. Create a dummy BMS chart
        let bms_content = "#TITLE FLAC Test\n#WAV01 01.wav\n#WAV02 02.wav\n#00111:0102";
        fs::write(song_dir.join("test.bms"), bms_content).unwrap();

        // 2. Generate two 16-bit stereo WAV files
        let wav1 = crate::flac::tests::make_test_wav_16bit(2, 44100, 22050);
        let wav2 = crate::flac::tests::make_test_wav_16bit(2, 44100, 11025);
        fs::write(song_dir.join("01.wav"), &wav1).unwrap();
        fs::write(song_dir.join("02.wav"), &wav2).unwrap();

        // 3. Pack with Turbo profile and Flac audio mode
        let opts = PackOptions::turbo(BgaPackMode::Embed).with_audio_mode(AudioPackMode::Flac);
        let output =
            pack_bms_folder_advanced_with_progress(&song_dir, None, opts, None, |_, _, _, _| {})
                .expect("flac packaging should succeed");

        let pkg = Package::from_bytes(output.base_package.clone()).expect("valid package");
        let manifest = pkg.manifest();
        let sound_meta = manifest.sound_atlas.as_ref().expect("sound atlas present");

        assert_eq!(sound_meta.codec, SoundAtlasCodec::FlacBundle);
        assert!(sound_meta.slices.contains_key("01.wav"));
        assert!(sound_meta.slices.contains_key("02.wav"));

        let atlas_bytes = pkg.read_entry(&sound_meta.file).expect("read atlas entry");
        assert!(
            atlas_bytes.len() < wav1.len() + wav2.len(),
            "FLAC bundle should compress audio"
        );

        // 4. Decode with beetle-audio SampleBank
        let chart = beetle_core::parse_bms(bms_content).expect("parse chart");
        let sample_bank =
            SampleBank::load_from_sound_atlas_for_chart(&chart, sound_meta, &atlas_bytes)
                .expect("decode soundbank");

        let pcm1 = sample_bank
            .get(beetle_core::WavId::new(1))
            .expect("wav01 present");
        let pcm2 = sample_bank
            .get(beetle_core::WavId::new(2))
            .expect("wav02 present");
        assert_eq!(pcm1.sample_rate, 44100);
        assert_eq!(pcm2.sample_rate, 44100);

        // Verify PCM matches original WAV
        let orig_pcm1 = SampleBank::load_audio_from_bytes(&wav1).expect("orig wav1");
        assert_eq!(pcm1.length, orig_pcm1.length);
        for i in 0..pcm1.length {
            let diff =
                (pcm1.samples[pcm1.offset + i] - orig_pcm1.samples[orig_pcm1.offset + i]).abs();
            assert!(diff < 1e-4, "Sample mismatch at {i}");
        }

        // 5. Export package to folder
        let pkg_path = temp_dir.join("flac_song.bmsp");
        fs::write(&pkg_path, &output.base_package).unwrap();

        let export_dir = temp_dir.join("Exported");
        let stats =
            export_package_to_folder(&pkg_path, &export_dir).expect("export should succeed");
        assert_eq!(stats.wav_files, 2);

        // 6. Test VFS mount
        let mut vfs = VirtualBmsFs::new();
        vfs.mount_package("flac_song", &pkg_path)
            .expect("mount should succeed");
        assert!(
            vfs.read_file("flac_song/01.wav").is_some()
                || vfs.read_file("flac_song/01.flac").is_some()
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
