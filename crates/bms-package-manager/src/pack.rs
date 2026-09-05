use crate::error::PackageManagerError;
use beetle_audio::{SampleBank, SoundAtlasBuilder};
use beetle_render::{is_video_path, BgaAtlasBuilder, ImageBuffer};
use bms_package::{Manifest, PackageBuilder, SoundAtlasCodec, MANIFEST_FILENAME};
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

/// Comprehensive options controlling package generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PackOptions {
    pub profile: PackProfile,
    pub bga_mode: BgaPackMode,
}

impl PackOptions {
    pub fn new(profile: PackProfile, bga_mode: BgaPackMode) -> Self {
        Self { profile, bga_mode }
    }

    pub fn classic(bga_mode: BgaPackMode) -> Self {
        Self {
            profile: PackProfile::Classic,
            bga_mode,
        }
    }

    pub fn turbo(bga_mode: BgaPackMode) -> Self {
        Self {
            profile: PackProfile::Turbo,
            bga_mode,
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

    let mut found_title = String::new();
    let mut found_artist = String::new();
    let mut found_genre = String::new();

    // Look for .bms, .bme, .bml, .pms files in target_dir
    for entry in fs::read_dir(target_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            if matches!(ext.as_str(), "bms" | "bme" | "bml" | "pms") {
                if let Ok(bytes) = fs::read(&path) {
                    let content = String::from_utf8_lossy(&bytes);
                    let (title, artist, genre) = extract_bms_header_tags(&content);
                    if !title.is_empty() && found_title.is_empty() {
                        found_title = title;
                    }
                    if !artist.is_empty() && found_artist.is_empty() {
                        found_artist = artist;
                    }
                    if !genre.is_empty() && found_genre.is_empty() {
                        found_genre = genre;
                    }
                }
            }
        }
    }

    let final_title = if !found_title.is_empty() {
        found_title
    } else {
        dir_name.to_string()
    };

    let final_artist = if !found_artist.is_empty() {
        found_artist
    } else {
        "Unknown".to_string()
    };

    let package_id = generate_slug_id(&final_artist, &final_title);

    let mut manifest = Manifest::new(package_id, final_title)
        .with_author(final_artist);

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
    pack_bms_folder_profile_with_progress(folder_path, manifest_override, profile, None, |_, _, _, _| {})
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
    pack_bms_folder_profile_with_progress(folder_path, manifest_override, PackProfile::Classic, cancel_flag, on_progress)
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
    let output = pack_bms_folder_advanced_with_progress(folder_path, manifest_override, options, cancel_flag, on_progress)?;
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
                let mut bga_manifest = Manifest::new_bga_companion(&bga_id, &bga_name, &manifest.id);
                bga_manifest.author = manifest.author.clone();

                let mut bga_builder = PackageBuilder::new(bga_manifest.clone());
                for (rel_path, data) in bga_files {
                    bga_builder.add_file(rel_path, data)?;
                }

                let bga_bytes = bga_builder.build_to_bytes_with_progress(cancel_flag, |curr, tot, name| {
                    on_progress("Compressing .bga.bmsp", curr, tot, name);
                })?;

                let bga_sha = bms_package::sha256_hex(&bga_bytes);
                let bga_size = bga_bytes.len() as u64;
                let rec_filename = format!("{}.bga.bmsp", manifest.id);
                let companion_info = bms_package::BgaCompanionInfo::new(rec_filename, bga_size, bga_sha);
                manifest = manifest.with_bga_companion(companion_info);

                bga_package_bytes = Some(bga_bytes);
                bga_manifest_opt = Some(bga_manifest);
            }

            let mut builder = PackageBuilder::new(manifest.clone());
            for (rel_path, data) in base_files {
                builder.add_file(rel_path, data)?;
            }

            let base_bytes = builder.build_to_bytes_with_progress(cancel_flag, |curr, tot, name| {
                on_progress("Compressing .bmsp", curr, tot, name);
            })?;

            Ok(PackOutput {
                base_package: base_bytes,
                bga_package: bga_package_bytes,
                base_manifest: manifest,
                bga_manifest: bga_manifest_opt,
            })
        }
        PackProfile::Turbo => {
            pack_bms_folder_turbo_with_progress(target_dir, manifest_override, options.bga_mode, cancel_flag, on_progress)
        }
    }
}

/// Turbo compilation: Assembles Sound Atlas and BGA Texture Atlas and builds a v2.0 package.
fn pack_bms_folder_turbo_with_progress<P: AsRef<Path>, F>(
    folder_path: P,
    manifest_override: Option<Manifest>,
    bga_mode: BgaPackMode,
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
    let mut manifest = match manifest_override {
        Some(m) => m,
        None => analyze_bms_folder(p)?,
    };

    let mut files_to_read = Vec::new();
    collect_file_paths(p, p, &mut files_to_read)?;

    // 1. Scan charts to map WavId and BmpId references to filenames
    let mut wav_targets: HashMap<String, String> = HashMap::new(); // norm_filename or norm_rel -> key ("01", "ZZ")
    let mut wav_stems: HashMap<String, String> = HashMap::new();   // stem -> key
    let mut bmp_targets: HashMap<String, String> = HashMap::new(); // norm_filename or norm_rel -> key ("01", "stagefile", "banner")
    let mut bmp_stems: HashMap<String, String> = HashMap::new();   // stem -> key

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
                    for (&wav_id, filename) in &chart.header.wav_table {
                        let key = beetle_core::encode_base36(wav_id);
                        let norm = filename.replace('\\', "/").to_ascii_lowercase();
                        let file_only = Path::new(&norm).file_name().and_then(|n| n.to_str()).unwrap_or(&norm).to_string();
                        let stem = Path::new(&file_only).file_stem().and_then(|s| s.to_str()).unwrap_or(&file_only).to_string();

                        wav_targets.insert(norm.clone(), key.clone());
                        wav_targets.insert(file_only.clone(), key.clone());
                        wav_stems.entry(stem.clone()).or_insert_with(|| key.clone());

                        // Cross-extension matching (.wav <-> .ogg)
                        if norm.ends_with(".wav") {
                            let base = &norm[..norm.len() - 4];
                            wav_targets.insert(format!("{}.ogg", base), key.clone());
                            let fbase = &file_only[..file_only.len() - 4];
                            wav_targets.insert(format!("{}.ogg", fbase), key.clone());
                        } else if norm.ends_with(".ogg") {
                            let base = &norm[..norm.len() - 4];
                            wav_targets.insert(format!("{}.wav", base), key.clone());
                            let fbase = &file_only[..file_only.len() - 4];
                            wav_targets.insert(format!("{}.wav", fbase), key.clone());
                        }
                    }
                    for (&bmp_id, filename) in &chart.header.bmp_table {
                        let key = beetle_core::encode_base36(beetle_core::WavId(bmp_id.0));
                        let norm = filename.replace('\\', "/").to_ascii_lowercase();
                        let file_only = Path::new(&norm).file_name().and_then(|n| n.to_str()).unwrap_or(&norm).to_string();
                        let stem = Path::new(&file_only).file_stem().and_then(|s| s.to_str()).unwrap_or(&file_only).to_string();

                        bmp_targets.insert(norm.clone(), key.clone());
                        bmp_targets.insert(file_only.clone(), key.clone());
                        bmp_stems.entry(stem.clone()).or_insert_with(|| key.clone());

                        for alt_ext in &["bmp", "png", "jpg", "jpeg"] {
                            bmp_targets.insert(format!("{}.{}", stem, alt_ext), key.clone());
                        }
                    }
                    if !chart.header.stage_file.is_empty() {
                        let norm = chart.header.stage_file.replace('\\', "/").to_ascii_lowercase();
                        let file_only = Path::new(&norm).file_name().and_then(|n| n.to_str()).unwrap_or(&norm).to_string();
                        let stem = Path::new(&file_only).file_stem().and_then(|s| s.to_str()).unwrap_or(&file_only).to_string();
                        bmp_targets.insert(norm, "stagefile".to_string());
                        bmp_targets.insert(file_only, "stagefile".to_string());
                        bmp_stems.entry(stem.clone()).or_insert_with(|| "stagefile".to_string());
                        for alt_ext in &["bmp", "png", "jpg", "jpeg"] {
                            bmp_targets.insert(format!("{}.{}", stem, alt_ext), "stagefile".to_string());
                        }
                    }
                    if !chart.header.banner.is_empty() {
                        let norm = chart.header.banner.replace('\\', "/").to_ascii_lowercase();
                        let file_only = Path::new(&norm).file_name().and_then(|n| n.to_str()).unwrap_or(&norm).to_string();
                        let stem = Path::new(&file_only).file_stem().and_then(|s| s.to_str()).unwrap_or(&file_only).to_string();
                        bmp_targets.insert(norm, "banner".to_string());
                        bmp_targets.insert(file_only, "banner".to_string());
                        bmp_stems.entry(stem.clone()).or_insert_with(|| "banner".to_string());
                        for alt_ext in &["bmp", "png", "jpg", "jpeg"] {
                            bmp_targets.insert(format!("{}.{}", stem, alt_ext), "banner".to_string());
                        }
                    }
                }
            }
        }
    }

    let ogg_count = files_to_read.iter().filter(|(rel, _)| {
        rel.to_ascii_lowercase().ends_with(".ogg")
    }).count();
    let wav_count = files_to_read.iter().filter(|(rel, _)| {
        rel.to_ascii_lowercase().ends_with(".wav")
    }).count();

    let sound_codec = if ogg_count > 0 && ogg_count >= wav_count {
        SoundAtlasCodec::OggBundle
    } else if wav_count > 0 {
        SoundAtlasCodec::WavBundle
    } else {
        SoundAtlasCodec::Pcm16
    };

    let is_bundle = sound_codec.is_bundle();
    let mut sound_builder = SoundAtlasBuilder::new(sound_codec).with_padding_frames(128);
    let mut bga_builder = BgaAtlasBuilder::new(1);
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
        let file_stem = Path::new(&norm_name).file_stem().and_then(|s| s.to_str()).unwrap_or(&norm_name);

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
        let matched_wav_key = wav_targets.get(&norm_rel)
            .or_else(|| wav_targets.get(&norm_name))
            .or_else(|| wav_stems.get(file_stem))
            .cloned();
        let is_audio = matches!(ext.as_str(), "wav" | "ogg") || matched_wav_key.is_some();
        if is_audio {
            let key = matched_wav_key.unwrap_or_else(|| {
                if file_stem.len() == 2 && beetle_core::decode_base36(file_stem.as_bytes()[0], file_stem.as_bytes()[1]).is_some() {
                    file_stem.to_ascii_uppercase()
                } else {
                    file_stem.to_string()
                }
            });

            if is_bundle {
                sound_builder.add_raw(key, data, Some(file_name.to_string()));
                continue;
            } else if let Ok(pcm) = SampleBank::load_audio_from_bytes(&data) {
                sound_builder.add_sample(key, &pcm, Some(file_name.to_string()));
                continue;
            }
        }

        // D. Image files -> Compile into BGA Texture Atlas
        let matched_bmp_key = bmp_targets.get(&norm_rel)
            .or_else(|| bmp_targets.get(&norm_name))
            .or_else(|| bmp_stems.get(file_stem))
            .cloned();
        let is_image = matches!(ext.as_str(), "bmp" | "png" | "jpg" | "jpeg") || matched_bmp_key.is_some();
        if is_image {
            if let Some(img) = ImageBuffer::from_bytes(&data) {
                let key = matched_bmp_key.unwrap_or_else(|| {
                    if file_stem.len() == 2 && beetle_core::decode_base36(file_stem.as_bytes()[0], file_stem.as_bytes()[1]).is_some() {
                        file_stem.to_ascii_uppercase()
                    } else {
                        file_stem.to_string()
                    }
                });
                bga_builder.add_frame(key, &img, Some(file_name.to_string()));
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

        let bga_bytes = bga_builder.build_to_bytes_with_progress(cancel_flag, |curr, tot, name| {
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
    let (sound_meta, sound_bytes) = sound_builder
        .build("audio/atlas.bin")
        .map_err(|e| PackageManagerError::InvalidPackage(format!("Sound Atlas build error: {e}")))?;
    manifest = manifest.with_sound_atlas(sound_meta);

    // 4. Build BGA Texture Atlas
    if let Some((bga_meta, bga_image)) = bga_builder.build("visual/atlas.bmp") {
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
            if file_name == MANIFEST_FILENAME || file_name.ends_with(".bmsp") || file_name.starts_with('.') {
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
        assert_eq!(
            generate_slug_id("Tatsh", "RED ZONE"),
            "tatsh.red_zone"
        );
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
        use bms_package::Package;
        use beetle_render::skin::ColorRgba;

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
        let pkg_bytes = pack_bms_folder_profile(&temp_dir, None, PackProfile::Turbo).expect("turbo pack failed");

        // 5. Inspect and verify package structure
        let pkg = Package::from_bytes(pkg_bytes).expect("package inspect failed");
        let manifest = pkg.manifest();

        assert_eq!(manifest.format, 2);
        assert!(manifest.sound_atlas.is_some());
        assert!(manifest.bga_atlas.is_some());

        let sound_atlas = manifest.sound_atlas.as_ref().unwrap();
        assert_eq!(sound_atlas.file, "audio/atlas.bin");
        // Should contain 01 and 02
        assert!(sound_atlas.slices.contains_key("01"));
        assert!(sound_atlas.slices.contains_key("02"));

        let bga_atlas = manifest.bga_atlas.as_ref().unwrap();
        assert_eq!(bga_atlas.file, "visual/atlas.bmp");
        assert!(bga_atlas.frames.contains_key("01"));
        assert!(bga_atlas.frames.contains_key("stagefile"));

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
        let atlas_bytes = pkg.read_entry("audio/atlas.bin").expect("read atlas bin failed");
        let sample_bank = SampleBank::load_from_sound_atlas(sound_atlas, &atlas_bytes)
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
        use bms_package::Package;
        use beetle_render::skin::ColorRgba;

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
        assert!(sound_meta.slices.contains_key("01"), "kick.wav matched");
        assert!(sound_meta.slices.contains_key("02"), "sound/snare.wav matched");

        let bga_meta = manifest.bga_atlas.as_ref().unwrap();
        assert!(bga_meta.frames.contains_key("01"), "bg.bmp matched");
        assert!(bga_meta.frames.contains_key("stagefile"), "stage.bmp matched");

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

        let pkg_bytes = pack_bms_folder_profile(&song_dir, None, PackProfile::Turbo).expect("pack failed");
        let pkg = Package::from_bytes(pkg_bytes).expect("package open failed");
        let manifest = pkg.manifest();

        assert_eq!(manifest.name, "Ogg Bundle Song");
        let sound_meta = manifest.sound_atlas.as_ref().expect("sound atlas missing");
        assert_eq!(sound_meta.codec, bms_package::SoundAtlasCodec::OggBundle);
        assert_eq!(sound_meta.slices.len(), 2);

        let slice1 = sound_meta.slices.get("01").unwrap();
        let slice2 = sound_meta.slices.get("02").unwrap();
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

        let pkg_bytes = pack_bms_folder_profile(&song_dir, None, PackProfile::Turbo).expect("pack failed");
        let pkg = Package::from_bytes(pkg_bytes).expect("package open failed");
        let manifest = pkg.manifest();

        assert_eq!(manifest.name, "Wav Bundle Song");
        let sound_meta = manifest.sound_atlas.as_ref().expect("sound atlas missing");
        assert_eq!(sound_meta.codec, bms_package::SoundAtlasCodec::WavBundle);
        assert_eq!(sound_meta.slices.len(), 2);

        let slice1 = sound_meta.slices.get("01").unwrap();
        let slice2 = sound_meta.slices.get("02").unwrap();
        assert_eq!(slice1.byte_len(), wav1_bytes.len() as u64);
        assert_eq!(slice2.byte_len(), wav2_bytes.len() as u64);

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
        let embed_out = pack_bms_folder_advanced_with_progress(&song_dir, None, embed_opts, None, |_, _, _, _| {})
            .expect("embed pack failed");
        assert!(embed_out.bga_package.is_none());
        assert!(embed_out.bga_manifest.is_none());
        assert!(embed_out.base_manifest.companion_packages.is_none());

        let embed_pkg = Package::from_bytes(embed_out.base_package).unwrap();
        assert!(embed_pkg.contains("bga.mp4"));
        assert!(embed_pkg.contains("movie.mpg"));

        // 2. Test BgaPackMode::NoVideo (Turbo)
        let novideo_opts = PackOptions::turbo(BgaPackMode::NoVideo);
        let novideo_out = pack_bms_folder_advanced_with_progress(&song_dir, None, novideo_opts, None, |_, _, _, _| {})
            .expect("novideo pack failed");
        assert!(novideo_out.bga_package.is_none());
        assert!(novideo_out.bga_manifest.is_none());
        assert!(novideo_out.base_manifest.companion_packages.is_none());

        let novideo_pkg = Package::from_bytes(novideo_out.base_package).unwrap();
        assert!(!novideo_pkg.contains("bga.mp4"));
        assert!(!novideo_pkg.contains("movie.mpg"));

        // 3. Test BgaPackMode::Split (Turbo)
        let split_opts = PackOptions::turbo(BgaPackMode::Split);
        let split_out = pack_bms_folder_advanced_with_progress(&song_dir, None, split_opts, None, |_, _, _, _| {})
            .expect("split pack failed");
        assert!(split_out.bga_package.is_some());
        assert!(split_out.bga_manifest.is_some());

        // Base package checks
        let base_pkg = Package::from_bytes(split_out.base_package).unwrap();
        assert!(!base_pkg.contains("bga.mp4"));
        assert!(!base_pkg.contains("movie.mpg"));
        assert_eq!(base_pkg.manifest().package_type, PackageType::Standard);

        // Companion metadata attached to base package
        let bga_comp = base_pkg.manifest().companion_packages.as_ref().unwrap().bga.as_ref().unwrap();
        assert_eq!(bga_comp.recommended_filename, format!("{}.bga.bmsp", base_pkg.manifest().id));

        // Companion package checks
        let bga_pkg_bytes = split_out.bga_package.unwrap();
        assert_eq!(bga_comp.size_bytes, bga_pkg_bytes.len() as u64);
        assert_eq!(bga_comp.sha256, bms_package::sha256_hex(&bga_pkg_bytes));

        let bga_pkg = Package::from_bytes(bga_pkg_bytes).unwrap();
        assert!(bga_pkg.contains("bga.mp4"));
        assert!(bga_pkg.contains("movie.mpg"));
        assert_eq!(bga_pkg.manifest().package_type, PackageType::BgaCompanion);
        assert_eq!(bga_pkg.manifest().target_package_id.as_deref(), Some(base_pkg.manifest().id.as_str()));

        // 4. Test BgaPackMode::Split (Classic)
        let classic_split_opts = PackOptions::classic(BgaPackMode::Split);
        let classic_out = pack_bms_folder_advanced_with_progress(&song_dir, None, classic_split_opts, None, |_, _, _, _| {})
            .expect("classic split pack failed");
        assert!(classic_out.bga_package.is_some());
        let classic_base = Package::from_bytes(classic_out.base_package).unwrap();
        assert!(!classic_base.contains("bga.mp4"));
        assert!(classic_base.contains("01.wav"));

        let classic_bga = Package::from_bytes(classic_out.bga_package.unwrap()).unwrap();
        assert!(classic_bga.contains("bga.mp4"));
        assert_eq!(classic_bga.manifest().package_type, PackageType::BgaCompanion);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
