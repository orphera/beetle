use beetle_audio::{PcmBuffer, SampleBank};
use beetle_core::{BmsChart, BmsHeader, Lane, NoteEvent, NoteType, WavId};

/// Creates a synthetic sample bank with synthesized drum and synth sounds for demo playback.
pub fn create_demo_sample_bank() -> SampleBank {
    let sample_rate = 44100;
    let mut bank = SampleBank::new();

    // 1. Kick Drum (WavId 1) - Pitch swept sine
    let kick_frames = 8820; // 0.2s
    let mut kick_samples = Vec::with_capacity(kick_frames * 2);
    for i in 0..kick_frames {
        let t = i as f32 / sample_rate as f32;
        let freq = 160.0 * (-t * 25.0).exp() + 45.0;
        let phase = 2.0 * std::f32::consts::PI * freq * t;
        let amp = (-t * 15.0).exp() * 0.8;
        let sample = (phase.sin() * amp).clamp(-1.0, 1.0);
        kick_samples.push(sample);
        kick_samples.push(sample);
    }
    bank.insert(WavId(1), PcmBuffer::new(sample_rate, kick_samples));

    // 2. Snare Drum (WavId 2) - Noise + tone
    let snare_frames = 6615; // 0.15s
    let mut snare_samples = Vec::with_capacity(snare_frames * 2);
    let mut noise_seed: u32 = 12345;
    for i in 0..snare_frames {
        let t = i as f32 / sample_rate as f32;
        noise_seed = noise_seed.wrapping_mul(1103515245).wrapping_add(12345);
        let noise = ((noise_seed >> 16) as f32 / 32768.0) - 1.0;
        let tone = (2.0 * std::f32::consts::PI * 220.0 * t).sin();
        let amp = (-t * 22.0).exp() * 0.6;
        let sample = ((noise * 0.7 + tone * 0.3) * amp).clamp(-1.0, 1.0);
        snare_samples.push(sample);
        snare_samples.push(sample);
    }
    bank.insert(WavId(2), PcmBuffer::new(sample_rate, snare_samples));

    // 3. Hi-Hat (WavId 3) - High-frequency noise
    let hat_frames = 2205; // 0.05s
    let mut hat_samples = Vec::with_capacity(hat_frames * 2);
    for i in 0..hat_frames {
        let t = i as f32 / sample_rate as f32;
        noise_seed = noise_seed.wrapping_mul(1103515245).wrapping_add(12345);
        let noise = ((noise_seed >> 16) as f32 / 32768.0) - 1.0;
        let amp = (-t * 60.0).exp() * 0.4;
        let sample = (noise * amp).clamp(-1.0, 1.0);
        hat_samples.push(sample);
        hat_samples.push(sample);
    }
    bank.insert(WavId(3), PcmBuffer::new(sample_rate, hat_samples));

    // 4. Synth Chord (WavId 4) - Chime tone
    let synth_frames = 17640; // 0.4s
    let mut synth_samples = Vec::with_capacity(synth_frames * 2);
    for i in 0..synth_frames {
        let t = i as f32 / sample_rate as f32;
        let f1 = (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        let f2 = (2.0 * std::f32::consts::PI * 554.37 * t).sin();
        let f3 = (2.0 * std::f32::consts::PI * 659.25 * t).sin();
        let amp = (-t * 6.0).exp() * 0.4;
        let sample = ((f1 + f2 + f3) * 0.33 * amp).clamp(-1.0, 1.0);
        synth_samples.push(sample);
        synth_samples.push(sample);
    }
    bank.insert(WavId(4), PcmBuffer::new(sample_rate, synth_samples));

    // 5. Scratch (WavId 5) - Pitch modulated wave
    let scratch_frames = 6615;
    let mut scratch_samples = Vec::with_capacity(scratch_frames * 2);
    for i in 0..scratch_frames {
        let t = i as f32 / sample_rate as f32;
        let freq = 200.0 + 300.0 * (2.0 * std::f32::consts::PI * 15.0 * t).sin();
        let amp = (-t * 12.0).exp() * 0.5;
        let sample = ((2.0 * std::f32::consts::PI * freq * t).sin() * amp).clamp(-1.0, 1.0);
        scratch_samples.push(sample);
        scratch_samples.push(sample);
    }
    bank.insert(WavId(5), PcmBuffer::new(sample_rate, scratch_samples));

    bank
}

/// Creates a demonstration BMS chart for instant gameplay testing.
pub fn create_demo_chart() -> BmsChart {
    let mut chart = BmsChart {
        header: BmsHeader {
            title: "Beetle Demo Track".to_string(),
            artist: "Beetle Sound Team".to_string(),
            genre: "Minimal Rhythm".to_string(),
            bpm: 150.0,
            play_level: 5,
            total: 300.0,
            ..Default::default()
        },
        ..Default::default()
    };

    // Construct an 8-measure pattern with notes and BGM
    for measure in 1..=8 {
        // BGM 4-on-the-floor kick + snare
        chart.bgm_notes.push((measure, 0.0, WavId(1)));
        chart.bgm_notes.push((measure, 0.25, WavId(3)));
        chart.bgm_notes.push((measure, 0.5, WavId(2)));
        chart.bgm_notes.push((measure, 0.75, WavId(3)));

        // Playable notes
        chart.notes.push(NoteEvent {
            measure,
            fraction: 0.0,
            lane: Lane::Key1,
            wav_id: Some(WavId(4)),
            note_type: NoteType::Tap,
        });

        chart.notes.push(NoteEvent {
            measure,
            fraction: 0.25,
            lane: Lane::Key3,
            wav_id: Some(WavId(4)),
            note_type: NoteType::Tap,
        });

        chart.notes.push(NoteEvent {
            measure,
            fraction: 0.5,
            lane: Lane::Key5,
            wav_id: Some(WavId(4)),
            note_type: NoteType::Tap,
        });

        chart.notes.push(NoteEvent {
            measure,
            fraction: 0.75,
            lane: Lane::Key7,
            wav_id: Some(WavId(4)),
            note_type: NoteType::Tap,
        });

        // Add occasional scratch notes
        if measure % 2 == 0 {
            chart.notes.push(NoteEvent {
                measure,
                fraction: 0.875,
                lane: Lane::Scratch,
                wav_id: Some(WavId(5)),
                note_type: NoteType::Tap,
            });
        }
    }

    // Sort notes
    chart.notes.sort_by(|a, b| {
        a.measure
            .cmp(&b.measure)
            .then_with(|| a.fraction.partial_cmp(&b.fraction).unwrap_or(std::cmp::Ordering::Equal))
    });

    chart
}
