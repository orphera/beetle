use std::fs;
use std::path::Path;

use beetle_render::ImageBuffer;
use bms_package::{PackageReader, SoundAtlasCodec};

use crate::error::PackageManagerError;

/// Synthesizes a standard 44-byte RIFF WAV header for 16-bit PCM audio.
pub fn create_riff_wav_header(
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    pcm_data_len: usize,
) -> [u8; 44] {
    let mut header = [0u8; 44];
    // ChunkID: "RIFF"
    header[0..4].copy_from_slice(b"RIFF");
    // ChunkSize: 36 + SubChunk2Size
    let chunk_size = (36 + pcm_data_len) as u32;
    header[4..8].copy_from_slice(&chunk_size.to_le_bytes());
    // Format: "WAVE"
    header[8..12].copy_from_slice(b"WAVE");
    // Subchunk1ID: "fmt "
    header[12..16].copy_from_slice(b"fmt ");
    // Subchunk1Size: 16 (for PCM)
    header[16..20].copy_from_slice(&16u32.to_le_bytes());
    // AudioFormat: 1 (PCM)
    header[20..22].copy_from_slice(&1u16.to_le_bytes());
    // NumChannels
    header[22..24].copy_from_slice(&channels.to_le_bytes());
    // SampleRate
    header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    // ByteRate: SampleRate * NumChannels * BitsPerSample / 8
    let byte_rate = sample_rate * (channels as u32) * (bits_per_sample as u32) / 8;
    header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    // BlockAlign: NumChannels * BitsPerSample / 8
    let block_align = channels * bits_per_sample / 8;
    header[32..34].copy_from_slice(&block_align.to_le_bytes());
    // BitsPerSample
    header[34..36].copy_from_slice(&bits_per_sample.to_le_bytes());
    // Subchunk2ID: "data"
    header[36..40].copy_from_slice(b"data");
    // Subchunk2Size: pcm_data_len
    header[40..44].copy_from_slice(&(pcm_data_len as u32).to_le_bytes());
    header
}

/// Extracts a synthesized standard WAV byte vector from a Sound Atlas binary buffer.
pub fn extract_wav_from_atlas(
    codec: SoundAtlasCodec,
    sample_rate: u32,
    atlas_data: &[u8],
    start_frame: u64,
    frame_count: u64,
) -> Option<Vec<u8>> {
    let channels = 2u16;
    let bits_per_sample = 16u16;

    let pcm16_bytes = match codec {
        SoundAtlasCodec::Pcm16 => {
            let start_byte = (start_frame as usize) * 4;
            let byte_len = (frame_count as usize) * 4;
            if start_byte + byte_len > atlas_data.len() {
                return None;
            }
            atlas_data[start_byte..start_byte + byte_len].to_vec()
        }
        SoundAtlasCodec::PcmF32 => {
            let start_byte = (start_frame as usize) * 8;
            let byte_len = (frame_count as usize) * 8;
            if start_byte + byte_len > atlas_data.len() {
                return None;
            }
            let slice = &atlas_data[start_byte..start_byte + byte_len];
            let sample_count = byte_len / 4;
            let mut pcm16 = Vec::with_capacity(sample_count * 2);
            for chunk in slice.chunks_exact(4) {
                let f = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                let s = (f.clamp(-1.0, 1.0) * 32767.0) as i16;
                pcm16.extend_from_slice(&s.to_le_bytes());
            }
            pcm16
        }
        SoundAtlasCodec::OggBundle | SoundAtlasCodec::WavBundle => {
            let start_byte = start_frame as usize;
            let byte_len = frame_count as usize;
            if start_byte + byte_len > atlas_data.len() {
                return None;
            }
            let slice = &atlas_data[start_byte..start_byte + byte_len];
            if slice.starts_with(b"RIFF") {
                return Some(slice.to_vec());
            }
            if let Ok(pcm) = beetle_audio::SampleBank::load_audio_from_bytes(slice) {
                let mut pcm16 = Vec::with_capacity(pcm.length * 2);
                for i in 0..pcm.length {
                    let s = (pcm.samples[pcm.offset + i].clamp(-1.0, 1.0) * 32767.0) as i16;
                    pcm16.extend_from_slice(&s.to_le_bytes());
                }
                let header = create_riff_wav_header(channels, sample_rate, bits_per_sample, pcm16.len());
                let mut wav = Vec::with_capacity(44 + pcm16.len());
                wav.extend_from_slice(&header);
                wav.extend_from_slice(&pcm16);
                return Some(wav);
            }
            return Some(slice.to_vec());
        }
    };

    let header = create_riff_wav_header(channels, sample_rate, bits_per_sample, pcm16_bytes.len());
    let mut wav = Vec::with_capacity(44 + pcm16_bytes.len());
    wav.extend_from_slice(&header);
    wav.extend_from_slice(&pcm16_bytes);
    Some(wav)
}

/// Statistics of exported files from a package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportStats {
    pub total_files: usize,
    pub bms_files: usize,
    pub wav_files: usize,
    pub bga_files: usize,
    pub video_files: usize,
    pub other_files: usize,
}

/// Exports a `.bmsp` package into a traditional BMS directory.
///
/// If the package uses Dual Atlas (Turbo), Sound Atlas slices are extracted as standard WAV files
/// and BGA Atlas frames are extracted as standard BMP files using preserved `original_filename`s.
pub fn export_package_to_folder<P: AsRef<Path>, Q: AsRef<Path>>(
    package_path: P,
    destination_dir: Q,
) -> Result<ExportStats, PackageManagerError> {
    export_package_to_folder_with_progress(package_path, destination_dir, |_, _, _, _| {})
}

/// Exports a `.bmsp` package with progress notification callback.
pub fn export_package_to_folder_with_progress<P: AsRef<Path>, Q: AsRef<Path>, F>(
    package_path: P,
    destination_dir: Q,
    mut on_progress: F,
) -> Result<ExportStats, PackageManagerError>
where
    F: FnMut(&str, usize, usize, &str),
{
    let mut pkg = PackageReader::open_file(package_path)?;
    let dest = destination_dir.as_ref();
    fs::create_dir_all(dest)?;

    let manifest = pkg.manifest().clone();
    let mut stats = ExportStats::default();

    let is_turbo = manifest.sound_atlas.is_some() || manifest.bga_atlas.is_some();

    if is_turbo {
        // 1. Extract Sound Atlas slices to WAV files
        if let Some(ref sound_meta) = manifest.sound_atlas {
            if let Ok(atlas_bytes) = pkg.read_entry(&sound_meta.file) {
                let total_slices = sound_meta.slices.len();
                for (idx, (key, slice)) in sound_meta.slices.iter().enumerate() {
                    let filename = slice
                        .original_filename
                        .as_deref()
                        .unwrap_or(key);

                    let filename = if !filename.to_lowercase().ends_with(".wav")
                        && !filename.to_lowercase().ends_with(".ogg")
                    {
                        if sound_meta.codec == SoundAtlasCodec::OggBundle {
                            format!("{}.ogg", filename)
                        } else {
                            format!("{}.wav", filename)
                        }
                    } else {
                        filename.to_string()
                    };

                    on_progress("Extracting keysounds", idx + 1, total_slices, &filename);

                    if sound_meta.codec.is_bundle() {
                        let start = slice.start_frame as usize;
                        let len = slice.frame_count as usize;
                        if start + len <= atlas_bytes.len() {
                            let out_path = dest.join(&filename);
                            if let Some(parent) = out_path.parent() {
                                let _ = fs::create_dir_all(parent);
                            }
                            fs::write(out_path, &atlas_bytes[start..start + len])?;
                            stats.wav_files += 1;
                            stats.total_files += 1;
                            continue;
                        }
                    }

                    if let Some(wav_data) = extract_wav_from_atlas(
                        sound_meta.codec,
                        sound_meta.sample_rate,
                        &atlas_bytes,
                        slice.start_frame,
                        slice.frame_count,
                    ) {
                        let out_path = dest.join(&filename);
                        if let Some(parent) = out_path.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        fs::write(out_path, wav_data)?;
                        stats.wav_files += 1;
                        stats.total_files += 1;
                    }
                }
            }
        }

        // 2. Extract BGA Atlas frames to BMP files
        if let Some(ref bga_meta) = manifest.bga_atlas {
            if let Ok(atlas_bytes) = pkg.read_entry(&bga_meta.file) {
                if let Some(atlas_img) = ImageBuffer::from_bytes(&atlas_bytes) {
                    let total_frames = bga_meta.frames.len();
                    for (idx, (key, frame)) in bga_meta.frames.iter().enumerate() {
                        let filename = frame
                            .original_filename
                            .as_deref()
                            .unwrap_or(key);

                        let filename = if !filename.to_lowercase().ends_with(".bmp")
                            && !filename.to_lowercase().ends_with(".png")
                            && !filename.to_lowercase().ends_with(".jpg")
                        {
                            format!("{}.bmp", filename)
                        } else {
                            filename.to_string()
                        };

                        on_progress("Extracting BGA frames", idx + 1, total_frames, &filename);

                        if let Some(sub_img) = atlas_img.crop(frame.x, frame.y, frame.width, frame.height) {
                            let bmp_bytes = sub_img.encode_bmp_bytes();
                            let out_path = dest.join(&filename);
                            if let Some(parent) = out_path.parent() {
                                let _ = fs::create_dir_all(parent);
                            }
                            fs::write(out_path, bmp_bytes)?;
                            stats.bga_files += 1;
                            stats.total_files += 1;
                        }
                    }
                }
            }
        }

        // 3. Extract non-atlas entries (charts, videos, text files)
        let entries: Vec<String> = pkg.entries().iter().map(|e| e.path.clone()).collect();
        let total_entries = entries.len();

        for (idx, entry_path) in entries.into_iter().enumerate() {
            if entry_path == bms_package::MANIFEST_FILENAME {
                continue;
            }
            if let Some(ref sm) = manifest.sound_atlas {
                if entry_path == sm.file {
                    continue;
                }
            }
            if let Some(ref bm) = manifest.bga_atlas {
                if entry_path == bm.file {
                    continue;
                }
            }

            on_progress("Extracting files", idx + 1, total_entries, &entry_path);

            if let Ok(data) = pkg.read_entry(&entry_path) {
                let out_path = dest.join(&entry_path);
                if let Some(parent) = out_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::write(&out_path, data)?;
                stats.total_files += 1;

                let ext = Path::new(&entry_path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();

                if matches!(ext.as_str(), "bms" | "bme" | "bml" | "pms") {
                    stats.bms_files += 1;
                } else if beetle_render::is_video_path(&entry_path) {
                    stats.video_files += 1;
                } else {
                    stats.other_files += 1;
                }
            }
        }
    } else {
        // Classic package: extract all files directly
        let entries: Vec<String> = pkg.entries().iter().map(|e| e.path.clone()).collect();
        let total_entries = entries.len();

        for (idx, entry_path) in entries.into_iter().enumerate() {
            if entry_path == bms_package::MANIFEST_FILENAME {
                continue;
            }

            on_progress("Extracting files", idx + 1, total_entries, &entry_path);

            if let Ok(data) = pkg.read_entry(&entry_path) {
                let out_path = dest.join(&entry_path);
                if let Some(parent) = out_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::write(&out_path, data)?;
                stats.total_files += 1;

                let ext = Path::new(&entry_path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();

                if matches!(ext.as_str(), "bms" | "bme" | "bml" | "pms") {
                    stats.bms_files += 1;
                } else if matches!(ext.as_str(), "wav" | "ogg") {
                    stats.wav_files += 1;
                } else if matches!(ext.as_str(), "bmp" | "png" | "jpg" | "jpeg") {
                    stats.bga_files += 1;
                } else if beetle_render::is_video_path(&entry_path) {
                    stats.video_files += 1;
                } else {
                    stats.other_files += 1;
                }
            }
        }
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_render::skin::ColorRgba;

    #[test]
    fn test_create_riff_wav_header_and_hound_decode() {
        let pcm = vec![0i16, 1000, 2000, -1000];
        let pcm_bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();

        let header = create_riff_wav_header(2, 44100, 16, pcm_bytes.len());
        let mut full_wav = Vec::new();
        full_wav.extend_from_slice(&header);
        full_wav.extend_from_slice(&pcm_bytes);

        // Verify with hound reader
        let cursor = std::io::Cursor::new(full_wav);
        let mut reader = hound::WavReader::new(cursor).expect("valid WAV header");
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_rate, 44100);
        assert_eq!(reader.spec().bits_per_sample, 16);

        let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(samples, pcm);
    }

    #[test]
    fn test_export_turbo_package_restores_all_files() {
        let temp_src = std::env::temp_dir().join(format!(
            "bpm_export_test_src_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let temp_dest = std::env::temp_dir().join(format!(
            "bpm_export_test_dest_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&temp_src).unwrap();

        // 1. Create chart, audio, and bga
        fs::write(temp_src.join("main.bms"), "#TITLE Export Test\n#WAV01 kick.wav\n#BMP01 bg.bmp\n").unwrap();

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
            w.write_sample(1000i16).unwrap();
            w.write_sample(-1000i16).unwrap();
            w.finalize().unwrap();
        }
        fs::write(temp_src.join("kick.wav"), cur.into_inner()).unwrap();

        let img = ImageBuffer::new(16, 16, ColorRgba::new(255, 128, 0, 255));
        fs::write(temp_src.join("bg.bmp"), img.encode_bmp_bytes()).unwrap();

        // 2. Pack as Turbo package
        let pkg_bytes = crate::pack_bms_folder_profile(&temp_src, None, crate::PackProfile::Turbo).unwrap();
        let pkg_path = temp_src.join("test.bmsp");
        fs::write(&pkg_path, pkg_bytes).unwrap();

        // 3. Export back to traditional BMS directory
        let stats = export_package_to_folder(&pkg_path, &temp_dest).expect("export failed");
        assert_eq!(stats.bms_files, 1);
        assert_eq!(stats.wav_files, 1);
        assert_eq!(stats.bga_files, 1);
        assert_eq!(stats.total_files, 3);

        // 4. Verify restored files on disk
        assert!(temp_dest.join("main.bms").exists());
        assert!(temp_dest.join("kick.wav").exists());
        assert!(temp_dest.join("bg.bmp").exists());

        // Verify exported WAV is valid and playable
        let wav_data = fs::read(temp_dest.join("kick.wav")).unwrap();
        let wav_reader = hound::WavReader::new(std::io::Cursor::new(wav_data)).unwrap();
        assert_eq!(wav_reader.spec().channels, 1);
        assert_eq!(wav_reader.spec().sample_rate, 44100);

        // Verify exported BMP is valid
        let bmp_data = fs::read(temp_dest.join("bg.bmp")).unwrap();
        let loaded_img = ImageBuffer::from_bytes(&bmp_data).expect("valid BMP");
        assert_eq!(loaded_img.width, 16);
        assert_eq!(loaded_img.height, 16);

        // Clean up
        let _ = fs::remove_dir_all(&temp_src);
        let _ = fs::remove_dir_all(&temp_dest);
    }
}
