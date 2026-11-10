//! Research helper: writes every chart of a GuitarFreaks & DrumMania disc
//! as BMS (`gfdm_bms`), `<out dir>/<id>_<part><difficulty>.bms`, and prints
//! the charts found.
//!
//! With an arrangement (`gfdm_arrange`, e.g. `6k+pitch+trill`), it writes
//! the drum, guitar and bass charts arranged by it instead,
//! `<out dir>/<id>_<part><difficulty>@<arrangement>.bms` (open charts are
//! not arranged). Song ids after it (comma separated) limit the songs.
//!
//! cargo run -p cs-disc --release --example gfdm_bms -- <image.iso> <out dir> [<arrangement> [<id>,...]]
use cs_disc::gfdm::{read_kits, Disc};
use cs_disc::gfdm_arrange::Arrange;
use cs_disc::gfdm_bms::{charts, to_bms, Song};
use cs_disc::Volume;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut d = cs_disc::open(&args[0]).unwrap();
    let out = std::path::Path::new(&args[1]);
    let arrange = args
        .get(2)
        .map(|a| Arrange::parse(a).expect("an arrangement such as 6k+pitch+trill"));
    let only: Option<Vec<u32>> = args
        .get(3)
        .map(|ids| ids.split(',').map(|n| n.parse().unwrap()).collect());
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume)
        .unwrap()
        .expect("no PACKINFO.BIN");
    let kits = read_kits(&mut d, &volume).unwrap().expect("kit tables");
    for song in disc.songs(&mut d).unwrap() {
        if only.as_ref().is_some_and(|o| !o.contains(&song.info.id)) {
            continue;
        }
        let c = disc.song_charts(&mut d, &song.info).unwrap();
        let s = Song {
            info: &c.info,
            drums: &c.drums,
            guitar: &c.guitar,
            kits: &kits,
            lanes: &c.lanes,
        };
        let list = charts(&c.drums, &c.guitar);
        let mut line = format!("{:4} {:15}", c.info.id, c.info.name);
        for chart in list {
            let chart = match arrange {
                Some(a) => match chart.arranged(a) {
                    Some(c) => c,
                    None => continue,
                },
                None => chart,
            };
            let bms = to_bms(&s, chart).unwrap();
            let key = chart.key(c.info.id).replace('/', "_");
            line += &format!(" {}", key.rsplit('_').next().unwrap());
            std::fs::write(out.join(format!("{key}.bms")), bms).unwrap();
        }
        println!("{line}");
    }
}
