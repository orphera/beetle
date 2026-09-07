use flacenc::bitsink::ByteSink;
use flacenc::component::BitRepr;
use flacenc::config::Encoder;
use flacenc::error::Verify;
use flacenc::source::MemSource;
use hound::{SampleFormat, WavReader};
use std::io::Cursor;

/// Encodes WAV PCM audio bytes into a lossless FLAC stream.
///
/// If the input is already a valid FLAC stream (starts with `fLaC`), it is returned as-is.
/// Supports 8-bit, 16-bit, 24-bit, and 32-bit (int or float) WAV files, normalizing
/// sample depths to FLAC-compatible representations (up to 24-bit).
pub fn encode_wav_to_flac(wav_bytes: &[u8]) -> Result<Vec<u8>, String> {
    if wav_bytes.starts_with(b"fLaC") {
        return Ok(wav_bytes.to_vec());
    }

    let mut reader = WavReader::new(Cursor::new(wav_bytes))
        .map_err(|e| format!("Failed to parse WAV header: {e}"))?;
    let spec = reader.spec();

    let channels = spec.channels as usize;
    let sample_rate = spec.sample_rate as usize;
    if channels == 0 || channels > 8 {
        return Err(format!("Unsupported channel count: {channels}"));
    }

    let (samples, bits_per_sample): (Vec<i32>, usize) = match spec.sample_format {
        SampleFormat::Float => {
            let float_samples: Vec<f32> = reader.samples::<f32>().filter_map(|s| s.ok()).collect();
            let int_samples = float_samples
                .into_iter()
                .map(|f| (f.clamp(-1.0, 1.0) * 8388607.0) as i32)
                .collect();
            (int_samples, 24)
        }
        SampleFormat::Int => {
            let bps = spec.bits_per_sample as usize;
            if bps <= 8 {
                let s: Vec<i32> = reader.samples::<i32>().filter_map(|s| s.ok()).collect();
                (s, 8)
            } else if bps <= 16 {
                let s: Vec<i32> = reader.samples::<i32>().filter_map(|s| s.ok()).collect();
                (s, 16)
            } else if bps <= 24 {
                let s: Vec<i32> = reader.samples::<i32>().filter_map(|s| s.ok()).collect();
                (s, 24)
            } else {
                // 32-bit int scaled down to 24-bit
                let s: Vec<i32> = reader
                    .samples::<i32>()
                    .filter_map(|s| s.ok())
                    .map(|v| v >> 8)
                    .collect();
                (s, 24)
            }
        }
    };

    if samples.is_empty() {
        return Err("WAV contains no audio samples".to_string());
    }

    let config = Encoder::default()
        .into_verified()
        .map_err(|e| format!("Failed to create verified FLAC encoder config: {e:?}"))?;

    let source = MemSource::from_samples(&samples, channels, bits_per_sample, sample_rate);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| format!("FLAC encoding failed: {e:?}"))?;

    let mut sink = ByteSink::with_capacity(stream.count_bits());
    stream
        .write(&mut sink)
        .map_err(|e| format!("FLAC bitstream write failed: {e:?}"))?;

    Ok(sink.into_inner())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use beetle_audio::SampleBank;
    use hound::{SampleFormat, WavSpec, WavWriter};

    pub fn make_test_wav_16bit(channels: u16, sample_rate: u32, num_frames: usize) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        let spec = WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        {
            let mut writer = WavWriter::new(&mut buffer, spec).unwrap();
            let total_samples = num_frames * channels as usize;
            for i in 0..total_samples {
                let val = ((i as f32 * 0.1).sin() * 20000.0) as i16;
                writer.write_sample(val).unwrap();
            }
            writer.finalize().unwrap();
        }
        buffer.into_inner()
    }

    #[test]
    fn test_encode_wav_to_flac_roundtrip() {
        let original_wav = make_test_wav_16bit(2, 44100, 44100);
        assert!(original_wav.len() > 80000);

        let flac_bytes = encode_wav_to_flac(&original_wav).expect("FLAC encoding should succeed");
        assert!(flac_bytes.starts_with(b"fLaC"));
        assert!(
            flac_bytes.len() < original_wav.len(),
            "FLAC ({}) should be smaller than WAV ({})",
            flac_bytes.len(),
            original_wav.len()
        );

        // Decode with beetle-audio SampleBank
        let pcm = SampleBank::load_audio_from_bytes(&flac_bytes)
            .expect("Should decode FLAC to PCM buffer");
        assert_eq!(pcm.sample_rate, 44100);
        assert_eq!(pcm.frame_count(), 44100);

        // Decode original WAV to PCM buffer and compare
        let pcm_wav = SampleBank::load_audio_from_bytes(&original_wav)
            .expect("Should decode original WAV");
        assert_eq!(pcm.length, pcm_wav.length);

        for i in 0..pcm.length {
            let diff = (pcm.samples[pcm.offset + i] - pcm_wav.samples[pcm_wav.offset + i]).abs();
            assert!(
                diff < 1e-4,
                "Sample mismatch at {i}: flac={}, wav={}",
                pcm.samples[pcm.offset + i],
                pcm_wav.samples[pcm_wav.offset + i]
            );
        }
    }
}
