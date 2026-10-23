//! Checks against the real IIDX 16 disc images, when they are on this machine.
//! Set `CS_DISC_IIDX16_DISC1` / `CS_DISC_IIDX16_DISC2` to the `.iso` paths;
//! without them these tests do nothing.

use cs_disc::iidx16::{Difficulty, Disc, Edition, Style};
use cs_disc::{identify, Game, SectorReader, Volume};

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
    assert_eq!(
        identify(&mut disc).unwrap(),
        Some(Game::Iidx16(Edition::PremiumBest))
    );
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
}

#[test]
fn disc1_is_empress() {
    let Some((mut disc, volume)) = open("CS_DISC_IIDX16_DISC1") else {
        return;
    };
    assert_eq!(
        identify(&mut disc).unwrap(),
        Some(Game::Iidx16(Edition::Empress))
    );
    let songs = Disc::open(&mut disc, &volume, Edition::Empress).unwrap();
    assert!(!songs.songs.is_empty());
}
