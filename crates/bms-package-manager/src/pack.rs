use crate::error::PackageManagerError;
use bms_package::{Manifest, PackageBuilder, MANIFEST_FILENAME};
use std::fs;
use std::path::Path;

/// Analyzes a directory containing BMS files to automatically generate a `Manifest`.
pub fn analyze_bms_folder<P: AsRef<Path>>(dir_path: P) -> Result<Manifest, PackageManagerError> {
    let p = dir_path.as_ref();
    if !p.is_dir() {
        return Err(PackageManagerError::InvalidPackage(format!(
            "'{}' is not a directory",
            p.display()
        )));
    }

    let dir_name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("bms_song");

    let mut found_title = String::new();
    let mut found_artist = String::new();
    let mut found_genre = String::new();

    // Look for .bms, .bme, .bml, .pms files in the directory
    for entry in fs::read_dir(p)? {
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

/// Packs an existing BMS directory into a standardized, deterministic `.bmsp` byte buffer.
pub fn pack_bms_folder<P: AsRef<Path>>(
    folder_path: P,
    manifest_override: Option<Manifest>,
) -> Result<Vec<u8>, PackageManagerError> {
    pack_bms_folder_with_progress(folder_path, manifest_override, None, |_, _, _, _| {})
}

/// Packs a BMS directory with cancellation check and progress reporting callback.
pub fn pack_bms_folder_with_progress<P: AsRef<Path>, F>(
    folder_path: P,
    manifest_override: Option<Manifest>,
    cancel_flag: Option<&std::sync::atomic::AtomicBool>,
    mut on_progress: F,
) -> Result<Vec<u8>, PackageManagerError>
where
    F: FnMut(&str, usize, usize, &str),
{
    let p = folder_path.as_ref();
    let manifest = match manifest_override {
        Some(m) => m,
        None => analyze_bms_folder(p)?,
    };

    let mut builder = PackageBuilder::new(manifest);
    let mut files_to_read = Vec::new();
    collect_file_paths(p, p, &mut files_to_read)?;

    let total = files_to_read.len();
    for (i, (rel_path, abs_path)) in files_to_read.into_iter().enumerate() {
        if let Some(flag) = cancel_flag {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(PackageManagerError::Cancelled);
            }
        }
        on_progress("Reading files", i + 1, total, &rel_path);
        let data = fs::read(&abs_path)?;
        builder.add_file(rel_path, data)?;
    }

    let bytes = builder.build_to_bytes_with_progress(cancel_flag, |curr, tot, name| {
        on_progress("Compressing .bmsp", curr, tot, name);
    })?;

    Ok(bytes)
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
}
