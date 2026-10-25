//! Checks against the real IIDX disc images, when they are on this machine.
//! Set `CS_DISC_IIDX16_DISC1` / `CS_DISC_IIDX16_DISC2` (and `CS_DISC_IIDX10` to
//! `CS_DISC_IIDX14` for the chart and sound checks) to the
//! `.iso` paths; without them these tests do nothing.

use cs_disc::iidx::{Difficulty, Disc, Style};
use cs_disc::Edition;
use cs_disc::{identify, SectorReader, Volume};

fn open(var: &str) -> Option<(SectorReader<std::fs::File>, Volume)> {
    let path = std::env::var(var).ok()?;
    let mut disc = SectorReader::open_image(path).expect("disc image opens");
    let volume = Volume::open(&mut disc).expect("ISO 9660 volume");
    Some((disc, volume))
}

#[test]
fn disc2_is_premium_best_and_lists_anisakis() {
    let Some((mut disc, volume)) = open("CS_DISC_IIDX16_DISC2") else {
        return;
    };
    assert_eq!(identify(&mut disc).unwrap(), Some(Edition::PremiumBest));
    let songs = Disc::open(&mut disc, &volume, Edition::PremiumBest).unwrap();
    assert_eq!(songs.songs.len(), 99);
    let anisakis = songs
        .songs
        .iter()
        .find(|s| s.title.eq_ignore_ascii_case("ANISAKIS"))
        .expect("ANISAKIS is on disc 2");
    let sp_normal = anisakis
        .charts
        .iter()
        .find(|c| c.style == Style::Single && c.difficulty == Difficulty::Normal)
        .expect("SP NORMAL");
    // Every chart file lies inside its DAT and reads whole.
    for song in &songs.songs {
        for chart in &song.charts {
            let bytes = songs.read_file(&mut disc, chart.file).unwrap();
            assert_eq!(bytes.len(), songs.file(chart.file).unwrap().size as usize);
        }
    }
    assert!(sp_normal.level > 0);
    // Background videos are MPEG program streams, stored as they are.
    let video = songs
        .read_file(&mut disc, anisakis.video.expect("ANISAKIS has a video"))
        .unwrap();
    assert_eq!(&video[..4], &[0, 0, 1, 0xba]);
    let with_video = songs.songs.iter().filter(|s| s.video.is_some()).count();
    assert!(with_video > 90, "{with_video} songs have a video");
}

#[test]
fn disc1_is_empress() {
    let Some((mut disc, volume)) = open("CS_DISC_IIDX16_DISC1") else {
        return;
    };
    assert_eq!(identify(&mut disc).unwrap(), Some(Edition::Empress));
    let songs = Disc::open(&mut disc, &volume, Edition::Empress).unwrap();
    assert!(!songs.songs.is_empty());
}

/// Every chart decrypts and decompresses to the `.ply` iidx-ps2tools extracted,
/// when that extraction is on this machine (`CS_DISC_IIDX16_PLY_DIR`: the
/// folder holding `<title> [<id>]/<title> [SP NORMAL].ply`).
#[test]
fn disc2_charts_match_the_extracted_ply_files() {
    let Some((mut disc, volume)) = open("CS_DISC_IIDX16_DISC2") else {
        return;
    };
    let Ok(ply_dir) = std::env::var("CS_DISC_IIDX16_PLY_DIR") else {
        return;
    };
    let songs = Disc::open(&mut disc, &volume, Edition::PremiumBest).unwrap();
    let mut compared = 0;
    for song in &songs.songs {
        for chart in &song.charts {
            let name = format!(
                "{}/{} [{:04}]/{} [{}].ply",
                ply_dir,
                song.title,
                song.id,
                song.title,
                slot_name(chart.style, chart.difficulty)
            );
            let ours = songs.read_chart(&mut disc, chart.file).unwrap();
            if let Ok(theirs) = std::fs::read(&name) {
                assert_eq!(ours, theirs, "{name}");
                compared += 1;
            }
        }
    }
    assert!(compared > 500, "only {compared} charts compared");
}

fn slot_name(style: Style, difficulty: Difficulty) -> String {
    let style = match style {
        Style::Single => "SP",
        Style::Double => "DP",
    };
    let difficulty = match difficulty {
        Difficulty::Beginner => "BEGINNER",
        Difficulty::Normal => "NORMAL",
        Difficulty::Hyper => "HYPER",
        Difficulty::Another => "ANOTHER",
        Difficulty::Black => "BLACK",
    };
    format!("{style} {difficulty}")
}

/// Every chart on both discs becomes BMS that the player parses, with every
/// played note of the `.ply` (all but the placeholders at time 0), each within
/// one frame of its time on the disc.
#[test]
fn every_chart_becomes_bms_with_its_notes_on_time() {
    use cs_disc::ply::{Command, Ply, Side};
    let mut worst_overall = Vec::new();
    for (var, edition) in [
        ("CS_DISC_IIDX10", Edition::TenthStyle),
        ("CS_DISC_IIDX11", Edition::Red),
        ("CS_DISC_IIDX12", Edition::HappySky),
        ("CS_DISC_IIDX13", Edition::Distorted),
        ("CS_DISC_IIDX14", Edition::Gold),
        ("CS_DISC_IIDX16_DISC1", Edition::Empress),
        ("CS_DISC_IIDX16_DISC2", Edition::PremiumBest),
    ] {
        let Some((mut disc, volume)) = open(var) else {
            continue;
        };
        let songs = Disc::open(&mut disc, &volume, edition).unwrap();
        let mut worst = (0.0f64, String::new());
        for song in &songs.songs {
            for chart in &song.charts {
                let what = format!("{} {:?} {:?}", song.title, chart.style, chart.difficulty);
                let bms = songs.chart_bms(&mut disc, song, chart, None).unwrap();
                let parsed =
                    beetle_core::parse_bms(&bms).unwrap_or_else(|e| panic!("{what}: {e:?}"));
                let timing = beetle_core::TimingModel::from_chart(&parsed);
                let ply = Ply::parse(&songs.read_chart(&mut disc, chart.file).unwrap()).unwrap();
                let double = chart.style == Style::Double;
                let mut expected: Vec<f64> = ply
                    .events
                    .iter()
                    .filter(|e| {
                        e.time > 0
                            && matches!(e.command, Command::Note { side, .. }
                                if side == Side::P1 || double)
                    })
                    .map(|e| ply.millis(e.time))
                    .collect();
                let mut got: Vec<f64> = parsed
                    .notes
                    .iter()
                    .map(|n| timing.beat_to_time_seconds(n.measure, n.fraction) * 1000.0)
                    .collect();
                assert_eq!(got.len(), expected.len(), "{what}: note count");
                expected.sort_by(f64::total_cmp);
                got.sort_by(f64::total_cmp);
                let chart_worst = got
                    .iter()
                    .zip(&expected)
                    .map(|(g, e)| (g - e).abs())
                    .fold(0.0, f64::max);
                if chart_worst > 1000.0 / 59.94 {
                    println!("  {what}: {chart_worst:.1} ms");
                }
                if chart_worst > worst.0 {
                    worst = (chart_worst, what.clone());
                }
            }
        }
        println!("{var}: worst note offset {:.2} ms ({})", worst.0, worst.1);
        worst_overall.push((var, worst));
    }
    for (var, worst) in worst_overall {
        assert!(worst.0 <= 1000.0 / 59.94, "{var}: {worst:?}");
    }
}

/// Every chart's key sound bank parses, every sound a chart plays (but 1, the
/// BGM cue) is in its bank and decodes, and every BGM stream decodes.
#[test]
fn every_chart_finds_and_decodes_its_sounds() {
    use cs_disc::ply::{Command, Ply};
    use cs_disc::wvb::{decode_bgm, Bank};
    for (var, edition) in [
        ("CS_DISC_IIDX10", Edition::TenthStyle),
        ("CS_DISC_IIDX11", Edition::Red),
        ("CS_DISC_IIDX12", Edition::HappySky),
        ("CS_DISC_IIDX13", Edition::Distorted),
        ("CS_DISC_IIDX14", Edition::Gold),
        ("CS_DISC_IIDX16_DISC1", Edition::Empress),
        ("CS_DISC_IIDX16_DISC2", Edition::PremiumBest),
    ] {
        let Some((mut disc, volume)) = open(var) else {
            continue;
        };
        let songs = Disc::open(&mut disc, &volume, edition).unwrap();
        let mut decoded = std::collections::BTreeSet::new();
        let mut checked = std::collections::BTreeSet::new();
        let mut banks = std::collections::BTreeMap::new();
        let mut missing = Vec::new();
        for song in &songs.songs {
            for chart in &song.charts {
                let what = format!("{} {:?} {:?}", song.title, chart.style, chart.difficulty);
                let (wvb, bgm) = song.sound_files(chart.style, chart.difficulty);
                let wvb = wvb.unwrap_or_else(|| panic!("{what}: no key sounds"));
                let bgm = bgm.unwrap_or_else(|| panic!("{what}: no BGM"));
                let data = banks
                    .entry(wvb)
                    .or_insert_with(|| songs.read_file(&mut disc, wvb).unwrap());
                let bank = Bank::parse(data).unwrap_or_else(|e| panic!("{what}: {e}"));
                let ply = Ply::parse(&songs.read_chart(&mut disc, chart.file).unwrap()).unwrap();
                for e in &ply.events {
                    let sound = match e.command {
                        Command::Sample { sound, .. } | Command::Auto { sound, .. } => sound,
                        _ => continue,
                    };
                    if sound <= 1 || !checked.insert((wvb, sound)) {
                        continue;
                    }
                    match bank.decode(sound) {
                        // Wrongly unscrambled frames decode to noise at full
                        // scale (RED's key sounds before its key was known).
                        Some(Ok(pcm)) => {
                            let full = pcm.channels[0]
                                .iter()
                                .filter(|s| s.unsigned_abs() >= 32767)
                                .count();
                            assert!(
                                full * 100 <= pcm.channels[0].len(),
                                "{what}: sound {sound} clips"
                            );
                        }
                        Some(Err(err)) => panic!("{what}: sound {sound}: {err}"),
                        None => missing.push(format!("{what}: sound {sound}")),
                    }
                }
                if decoded.insert(bgm) {
                    let data = songs.read_file(&mut disc, bgm).unwrap();
                    let pcm = decode_bgm(&data).unwrap_or_else(|e| panic!("{what}: BGM {e}"));
                    // Songs played all from key sounds (DistorteD's DUE
                    // TOMORROW and DRUNK MONKY, GOLD's SH) hold a stub of
                    // under a second at volume 1 in the BGM's place.
                    let stub = data[40] <= 1;
                    assert!(
                        stub || pcm.channels[0].len() as u32 > pcm.rate * 10,
                        "{what}: BGM too short"
                    );
                }
            }
        }
        missing.dedup();
        assert!(
            missing.is_empty(),
            "{var}: {} missing, first {:?}",
            missing.len(),
            &missing[..missing.len().min(10)]
        );
    }
}

/// Every pop'n chart becomes BMS the player reads as nine buttons, with every
/// note of the chart on time; every sound a chart plays is in its bank and
/// decodes, every BGM decodes, and every song has its banner. Set
/// `CS_DISC_POPN12` to `CS_DISC_POPN14`.
#[test]
fn every_popn_chart_plays_nine_buttons_with_its_sounds() {
    use cs_disc::ply::Command;
    use cs_disc::wvb::{decode_bgm, Bank};
    for (var, edition) in [
        ("CS_DISC_POPN12", Edition::Popn12),
        ("CS_DISC_POPN13", Edition::Popn13),
        ("CS_DISC_POPN14", Edition::Popn14),
    ] {
        let Some((mut disc, volume)) = open(var) else {
            continue;
        };
        assert_eq!(identify(&mut disc).unwrap(), Some(edition));
        let songs = Disc::open(&mut disc, &volume, edition).unwrap();
        let mut worst = (0.0f64, String::new());
        let mut missing = Vec::new();
        let mut checked = std::collections::BTreeSet::new();
        for song in &songs.songs {
            // The song's banner (genre and title) is its loading picture.
            let banner = songs
                .stage_image(&mut disc, song, false)
                .unwrap()
                .unwrap_or_else(|| panic!("{}: no banner", song.title));
            // About 244x58 (13's アイドルポップ is 242 wide, 12's クラシック１１ 59 high).
            assert!(
                (236..=248).contains(&banner.width) && (56..=60).contains(&banner.height),
                "{}: {}x{}",
                song.title,
                banner.width,
                banner.height
            );
            for chart in &song.charts {
                let what = format!("{} {} {:?}", song.id, song.title, chart.difficulty);
                let (wvb, bgm) = song.sound_files(chart.style, chart.difficulty);
                let (wvb, bgm) = (wvb.unwrap(), bgm.unwrap());
                let bank_bytes = songs.read_file(&mut disc, wvb).unwrap();
                let bank = Bank::parse(&bank_bytes).unwrap_or_else(|e| panic!("{what}: {e}"));
                assert!(
                    bank.numbers().all(|n| n < edition.bgm_sound()),
                    "{what}: a key sound takes the BGM's number"
                );
                if checked.insert(bgm) {
                    let bytes = songs.read_file(&mut disc, bgm).unwrap();
                    let pcm = decode_bgm(&bytes).unwrap_or_else(|e| panic!("{what}: BGM {e}"));
                    assert!(pcm.channels[0].len() > 44100, "{what}: BGM too short");
                }
                let ply = songs.chart_ply(&mut disc, chart).unwrap();
                for e in &ply.events {
                    if let Command::Sample { sound, .. } | Command::Auto { sound, .. } = e.command {
                        if sound != 0
                            && sound != edition.bgm_sound()
                            && bank.decode(sound).is_none_or(|r| r.is_err())
                        {
                            missing.push(format!("{what}: sound {sound}"));
                        }
                    }
                }
                let bms = songs.chart_bms(&mut disc, song, chart, None).unwrap();
                let parsed =
                    beetle_core::parse_bms(&bms).unwrap_or_else(|e| panic!("{what}: {e:?}"));
                assert_eq!(
                    parsed.detect_play_mode_with_hint(false),
                    beetle_core::PlayMode::Keys9,
                    "{what}"
                );
                let timing = beetle_core::TimingModel::from_chart(&parsed);
                let mut expected: Vec<f64> = ply
                    .events
                    .iter()
                    .filter(|e| matches!(e.command, Command::Note { .. }))
                    .map(|e| ply.millis(e.time))
                    .collect();
                let mut got: Vec<f64> = parsed
                    .notes
                    .iter()
                    .map(|n| timing.beat_to_time_seconds(n.measure, n.fraction) * 1000.0)
                    .collect();
                assert_eq!(got.len(), expected.len(), "{what}: note count");
                expected.sort_by(f64::total_cmp);
                got.sort_by(f64::total_cmp);
                let off = got
                    .iter()
                    .zip(&expected)
                    .map(|(g, e)| (g - e).abs())
                    .fold(0.0, f64::max);
                if off > 5.0 {
                    println!("  {what}: {off:.1} ms");
                }
                if off > worst.0 {
                    worst = (off, what);
                }
            }
        }
        missing.dedup();
        println!(
            "{var}: {} songs, worst note offset {:.2} ms ({}), {} missing sounds",
            songs.songs.len(),
            worst.0,
            worst.1,
            missing.len()
        );
        for m in missing.iter().take(10) {
            println!("  {m}");
        }
        assert!(missing.is_empty(), "{var}: sounds missing");
        assert!(worst.0 <= 10.0, "{var}: {worst:?}");
    }
}
