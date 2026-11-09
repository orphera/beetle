//! Research helper: writes every chart of a GuitarFreaks & DrumMania disc
//! as BMS (`gfdm_bms`), `<out dir>/<id>_<part><difficulty>.bms`, and prints
//! the charts found.
//!
//! cargo run -p cs-disc --release --example gfdm_bms -- <image.iso> <out dir>
use cs_disc::gfdm::{read_kits, Disc};
use cs_disc::gfdm_bms::{charts, to_bms, Song};
use cs_disc::Volume;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut d = cs_disc::open(&args[0]).unwrap();
    let out = std::path::Path::new(&args[1]);
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume)
        .unwrap()
        .expect("no PACKINFO.BIN");
    let kits = read_kits(&mut d, &volume).unwrap().expect("kit tables");
    for song in disc.songs(&mut d).unwrap() {
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
            let bms = to_bms(&s, chart).unwrap();
            let key = chart.key(c.info.id).replace('/', "_");
            line += &format!(" {}", key.rsplit('_').next().unwrap());
            std::fs::write(out.join(format!("{key}.bms")), bms).unwrap();
        }
        println!("{line}");
    }
}
