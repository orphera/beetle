use beetle_core::{ChartId, TableIndex};
use bms_package_manager::base_match;
use bms_package_manager::collection::{self, LocationKind};
use bms_package_manager::songs;
use bms_package_manager::table_ops;
use bms_package_manager::{
    absolute_dir, fetch_table, find_available_updates, load_library, save_library, HttpClient,
    PackageManager, PackageManagerError, PackageUpdater, RegistryCacheManager, RegistrySource,
    RemotePackageInstaller, RemoteRegistryIndex, SourcesConfig, TableStore, UpdateOutcome,
};
use bms_package_manager::{ir, table_fetch};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn print_usage() {
    println!("BMS Package Manager (bpm)");
    println!();
    println!("Usage:");
    println!("  bpm scan                               Index registered folders and installed packages (collection.idx)");
    println!("  bpm status                             Summarize the collection index: charts, copies, duplicates");
    println!("  bpm dupes [--kind folder|mixed]        List charts that exist in more than one place, with the copy that loads");
    println!("  bpm songs [filters]                    List songs (charts grouped by title, artist and key sounds)");
    println!("  bpm song <id|title>                    Show one song: its charts, copies, the copy that loads, candidates");
    println!("  bpm song link|unlink <chart>...        Group charts by hand, or split a chart from its song for good");
    println!("  bpm charts [filters]                   List charts with their song, mode, level and copies");
    println!("  bpm install <package.bmsp_or_id> [--with-bga] Install local package or download from remote registry");
    println!("  bpm update [delta.bmdp]                Update remote registry indexes (or apply a delta package)");
    println!("  bpm search <query>                     Search remote packages across configured registries");
    println!("  bpm upgrade                            Batch upgrade installed packages to latest remote versions");
    println!(
        "  bpm source <list|add|remove>           Manage remote registry sources (sources.json)"
    );
    println!("  bpm table <add|update|list|remove>     Manage difficulty tables (tables/, read by the player)");
    println!("  bpm library <add|list|remove> [path]   Register existing BMS folders the player scans in place (no copy)");
    println!("  bpm import <folder_or_zip>             Import BMS folder(s) or a zip of BMS folders into managed storage");
    println!("  bpm pack <folder> [-o <out>] [--turbo] [--flac] [--split-bga] [--no-video] Pack a BMS folder into a .bmsp archive");
    println!("  bpm diff <base> <target> [-o <out>]    Generate a .bmdp delta package between states/folders");
    println!(
        "  bpm patch <base> <diff> [-o <out>]     Reconstruct a target .bmsp from base + diff"
    );
    println!("  bpm export <package_or_id> [-o <dir>]  Export package back into traditional BMS folder structure");
    println!(
        "  bpm bga install <package.bga.bmsp_or_id> Install a decoupled BGA companion package"
    );
    println!("  bpm bga remove <package_id>            Remove BGA companion from package to save disk space");
    println!("  bpm bga status <package_id>            Check BGA status of an installed package");
    println!("  bpm mount [--port <port>] [--drive <Z:>] Mount packages onto on-the-fly virtual VFS drive");
    println!("  bpm unmount [--drive <Z:>]             Unmount virtual VFS network drive");
    println!("  bpm list                               List all active installed packages");
    println!("  bpm info <package_id>                  Show package metadata and installed states");
    println!("  bpm states <package_id>                List all installed states of a package");
    println!("  bpm activate <id> <state_hash>         Switch active state for a package");
    println!("  bpm uninstall <id> <state_hash>        Uninstall a specific package state");
    println!("  bpm serve [--port <port>] [--bind <addr>] Host local package storage as a LAN registry hub");
}

fn print_progress_bar(label: &str, current: u64, total: Option<u64>) {
    use std::io::Write;
    let mb_cur = current as f64 / (1024.0 * 1024.0);
    if let Some(tot) = total {
        if tot > 0 {
            let pct = ((current as f64 / tot as f64) * 100.0).clamp(0.0, 100.0);
            let mb_tot = tot as f64 / (1024.0 * 1024.0);
            let bar_len = 25;
            let filled = ((pct / 100.0) * bar_len as f64).round() as usize;
            let filled = filled.min(bar_len);
            let bar_filled = "=".repeat(filled);
            let arrow = if filled < bar_len { ">" } else { "" };
            let empty_len = bar_len.saturating_sub(filled + arrow.len());
            let bar_empty = " ".repeat(empty_len);
            print!(
                "\r{}[{}{}{}] {:>3.0}% ({:.2} MB / {:.2} MB)",
                label, bar_filled, arrow, bar_empty, pct, mb_cur, mb_tot
            );
            let _ = std::io::stdout().flush();
            return;
        }
    }
    print!("\r{}[downloading] {:.2} MB", label, mb_cur);
    let _ = std::io::stdout().flush();
}

fn get_default_packages_dir() -> PathBuf {
    bms_package::installed::packages_root()
}

fn print_table_usage() {
    println!("Usage:");
    println!("  bpm table add <address>             Install a difficulty table from its page or header.json");
    println!("  bpm table update [name] [--force]   Fetch installed tables again (all, or one)");
    println!("  bpm table list                      List installed tables");
    println!("  bpm table missing [name]            Charts in the tables the collection lacks (after `bpm scan`)");
    println!("  bpm table fetch <name> <#>... [--yes]  Download the difference packs of the chosen entries (direct links only)");
    println!("  bpm table fetch <name> <#>... --body   Open the body pages of the chosen entries in the browser");
    println!("  bpm table fetch <name> <#>... --ir     Download the body and diff links that the Stella IR chart page lists");
    println!("  bpm table get <name> <#> [--body-file <file>] [--yes]  Get one entry: body file + difference, imported as a package");
    println!("  bpm table base <name> <#>           Guess which owned folder is the original of an entry's difference (after `bpm scan`)");
    println!("  bpm table remove <name>             Delete an installed table");
    println!();
    println!("Tables are kept in ./tables (or $BEETLE_TABLES_DIR), where the player reads them.");
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "3 days ago", "5 hours ago", "just now".
fn ago(fetched: u64, now: u64) -> String {
    if fetched == 0 {
        return "never".to_string();
    }
    let secs = now.saturating_sub(fetched);
    match secs {
        0..=89 => "just now".to_string(),
        90..=5399 => format!("{} minutes ago", secs / 60),
        5400..=129_599 => format!("{} hours ago", secs / 3600),
        _ => format!("{} days ago", secs / 86_400),
    }
}

fn exit_with(message: &dyn std::fmt::Display) -> ! {
    eprintln!("Error: {message}");
    std::process::exit(1);
}

/// The index `bpm scan` wrote, or exit with a hint to scan first.
fn load_index_or_exit() -> collection::Index {
    let path = collection::index_file();
    let text = fs::read_to_string(&path).unwrap_or_default();
    collection::Index::parse(&text).unwrap_or_else(|| {
        exit_with(&format!(
            "no usable index at {} (run `bpm scan` first)",
            path.display()
        ))
    })
}

fn run_scan(manager: &PackageManager) {
    let report = table_ops::scan_collection(manager.root_dir()).unwrap_or_else(|e| exit_with(&e));
    let path = collection::index_file();
    println!(
        "Scanned {} folder(s) and {} active package state(s).",
        report.folders, report.package_states
    );
    println!(
        "  copies: {}   charts: {}   duplicate charts: {}   skipped: {}",
        report.copies, report.charts, report.duplicate_groups, report.skipped
    );
    println!("  index: {}", path.display());
}

fn distinct_charts(index: &collection::Index) -> usize {
    index
        .locations
        .iter()
        .map(|l| l.chart)
        .collect::<BTreeSet<_>>()
        .len()
}

fn run_status() {
    let index = load_index_or_exit();
    let folders = load_library().paths().len();
    let package_states: BTreeSet<&str> = index
        .locations
        .iter()
        .filter(|l| l.kind == LocationKind::Package)
        .map(|l| l.source.as_str())
        .collect();
    let groups = index.duplicate_groups();
    let extra_copies: usize = groups.iter().map(|g| g.copies.len() - 1).sum();
    let packaged: BTreeSet<_> = index
        .locations
        .iter()
        .filter(|l| l.kind == LocationKind::Package)
        .map(|l| l.chart)
        .collect();

    println!(
        "Index:      {} (scanned {}, key_v {})",
        collection::index_file().display(),
        ago(index.scanned_at, now_secs()),
        collection::KEY_VERSION
    );
    println!(
        "Sources:    {folders} folder(s), {} active package state(s)",
        package_states.len()
    );
    println!(
        "Charts:     {} distinct, {} copies",
        distinct_charts(&index),
        index.locations.len()
    );
    println!(
        "Duplicates: {} chart(s) in more than one place, {extra_copies} extra copies",
        groups.len()
    );
    println!(
        "Packaged:   {} chart(s) have a package copy",
        packaged.len()
    );
    println!(
        "Skipped:    {} unreadable chart(s) at last scan",
        index.skipped
    );
    let songs_count = songs::build(&index, &load_manual()).songs.len();
    println!("Songs:      {songs_count}");
}

fn run_dupes(args: &[String]) {
    let filter = match args {
        [] => None,
        [flag, kind] if flag == "--kind" => match kind.as_str() {
            "folder" | "mixed" => Some(kind.as_str()),
            other => exit_with(&format!("unknown --kind '{other}' (use folder or mixed)")),
        },
        _ => exit_with(&"usage: bpm dupes [--kind folder|mixed]"),
    };
    let index = load_index_or_exit();
    let groups: Vec<_> = index
        .duplicate_groups()
        .into_iter()
        .filter(|g| match filter {
            Some("folder") => g.package_copies() == 0,
            Some(_) => g.folder_copies() > 0 && g.package_copies() > 0,
            None => true,
        })
        .collect();

    println!("Charts in more than one place: {}", groups.len());
    println!("Only byte-identical copies are matched; same content in different files is not.");
    for group in &groups {
        let first = group.copies[0];
        println!();
        println!(
            "{}  {} / {}  {} lv{}  ({} folder, {} package)",
            group.chart.short(),
            first.title,
            first.artist,
            first.mode,
            first.play_level,
            group.folder_copies(),
            group.package_copies()
        );
        let loaded = collection::Index::load_index(&group.copies);
        for (i, copy) in group.copies.iter().enumerate() {
            let marker = if Some(i) == loaded { "*" } else { " " };
            println!(
                "  {marker} {}  missing {} key sound(s)",
                describe_location(copy),
                copy.missing_keys
            );
        }
        match loaded {
            Some(i) if group.copies[i].is_intact() => {
                println!(
                    "    loads: {} (first intact copy)",
                    describe_location(group.copies[i])
                )
            }
            Some(i) => println!(
                "    loads: {} (no intact copy; key sounds missing)",
                describe_location(group.copies[i])
            ),
            None => {}
        }
    }
}

#[derive(Default)]
struct ChartFilter {
    mode: Option<String>,
    level: Option<u32>,
    packaged: Option<bool>,
    dupes: bool,
    /// A table's name or symbol, or a level chip such as `sl3`.
    table: Option<String>,
}

fn parse_chart_filter(args: &[String]) -> ChartFilter {
    const USAGE: &str =
        "usage: [--mode 7k] [--level N] [--table sl|sl3] [--packaged|--unpackaged] [--dupes]";
    let mut filter = ChartFilter::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--mode" => {
                let value = it.next().unwrap_or_else(|| exit_with(&USAGE));
                let mode = normalize_mode(value);
                if !KNOWN_MODES.contains(&mode.as_str()) {
                    exit_with(&format!(
                        "unknown mode '{value}' (use one of {})",
                        KNOWN_MODES.join(" ")
                    ));
                }
                filter.mode = Some(mode);
            }
            "--level" => {
                let value = it.next().unwrap_or_else(|| exit_with(&USAGE));
                filter.level = Some(value.parse().unwrap_or_else(|_| {
                    exit_with(&format!("--level expects a number, got '{value}'"))
                }));
            }
            "--table" => {
                let value = it.next().unwrap_or_else(|| exit_with(&USAGE));
                filter.table = Some(value.to_ascii_lowercase());
            }
            "--packaged" => filter.packaged = Some(true),
            "--unpackaged" => filter.packaged = Some(false),
            "--dupes" => filter.dupes = true,
            _ => exit_with(&USAGE),
        }
    }
    filter
}

/// Modes as `normalize_mode` writes them, in the order the game lists them.
const KNOWN_MODES: &[&str] = &["4K", "5K", "6K", "7K", "8K", "9K", "10K", "14K"];

/// `7k`, `7K` and `7KEYS` all name the same mode.
fn normalize_mode(text: &str) -> String {
    text.to_ascii_uppercase().replace("KEYS", "K")
}

/// The installed difficulty tables, matched against the charts in the index.
fn load_tables(index: &collection::Index) -> TableIndex {
    let dir = env::var("BEETLE_TABLES_DIR").unwrap_or_else(|_| "tables".to_string());
    let tables: Vec<_> = TableStore::new(dir)
        .list()
        .into_iter()
        .map(|(_, table)| table)
        .collect();
    let mut matched = TableIndex::new(tables);
    matched.match_songs(index.locations.iter().map(|l| (l.chart, l.md5)));
    matched
}

/// Whether the chart is in a table whose name, symbol, or `symbol+level` is `query`.
fn in_table(tables: &TableIndex, chart: ChartId, query: &str) -> bool {
    tables.matches_for(chart).iter().any(|m| {
        let table = &tables.tables()[m.table];
        let entry = &table.entries[m.entry];
        table.name.eq_ignore_ascii_case(query)
            || table.symbol.eq_ignore_ascii_case(query)
            || format!("{}{}", table.symbol, entry.level).eq_ignore_ascii_case(query)
    })
}

/// Level chips of a chart in every table that has it (`sl3 st2`), or `-`.
fn table_chips(tables: &TableIndex, chart: ChartId) -> String {
    let chips: Vec<String> = tables
        .matches_for(chart)
        .iter()
        .map(|m| {
            let table = &tables.tables()[m.table];
            format!("{}{}", table.symbol, table.entries[m.entry].level)
        })
        .collect();
    if chips.is_empty() {
        "-".to_string()
    } else {
        chips.join(" ")
    }
}

fn chart_matches(record: &songs::ChartRecord, filter: &ChartFilter, tables: &TableIndex) -> bool {
    filter
        .mode
        .as_ref()
        .is_none_or(|m| *m == normalize_mode(&record.mode))
        && filter.level.is_none_or(|l| l == record.play_level)
        && filter.packaged.is_none_or(|p| p == record.is_packaged())
        && (!filter.dupes || record.copies.len() > 1)
        && filter
            .table
            .as_ref()
            .is_none_or(|query| in_table(tables, record.chart, query))
}

fn run_songs(args: &[String]) {
    let filter = parse_chart_filter(args);
    let index = load_index_or_exit();
    let tables = load_tables(&index);
    check_table_filter(&filter, &tables);
    let collection = songs::build(&index, &load_manual());

    let rows: Vec<_> = collection
        .songs
        .iter()
        .filter_map(|song| {
            let matching = song
                .charts
                .iter()
                .filter(|&&i| chart_matches(&collection.records[i], &filter, &tables))
                .count();
            (matching > 0).then_some((song, matching))
        })
        .collect();
    println!(
        "{:<10} {:<34} {:<22} {:>7} {:>7}  PACKAGED",
        "SONG", "TITLE", "ARTIST", "CHARTS", "COPIES"
    );
    println!("{:-<90}", "");
    for (song, matching) in &rows {
        let copies: usize = song
            .charts
            .iter()
            .map(|&i| collection.records[i].copies.len())
            .sum();
        let packaged = song
            .charts
            .iter()
            .filter(|&&i| collection.records[i].is_packaged())
            .count();
        println!(
            "{:<10} {:<34} {:<22} {:>7} {:>7}  {}",
            song.id,
            truncate(&song.title, 34),
            truncate(&song.artist, 22),
            format!("{matching}/{}", song.charts.len()),
            copies,
            packaged
        );
    }
    println!("{} song(s)", rows.len());
}

fn run_charts(args: &[String]) {
    let filter = parse_chart_filter(args);
    let index = load_index_or_exit();
    let tables = load_tables(&index);
    check_table_filter(&filter, &tables);
    let collection = songs::build(&index, &load_manual());
    let mut song_of = vec![String::new(); collection.records.len()];
    for song in &collection.songs {
        for &i in &song.charts {
            song_of[i] = song.id.clone();
        }
    }

    let mut shown = 0;
    println!(
        "{:<10} {:<17} {:<7} {:>3} {:<12} {:>7}  TITLE / ARTIST",
        "SONG", "CHART", "MODE", "LV", "TABLE", "COPIES"
    );
    println!("{:-<100}", "");
    for (i, record) in collection.records.iter().enumerate() {
        if !chart_matches(record, &filter, &tables) {
            continue;
        }
        shown += 1;
        let folders = record
            .copies
            .iter()
            .filter(|c| c.kind == LocationKind::Folder)
            .count();
        println!(
            "{:<10} {:<17} {:<7} {:>3} {:<12} {:>7}  {} / {}",
            song_of[i],
            record.chart.short(),
            normalize_mode(&record.mode),
            record.play_level,
            truncate(&table_chips(&tables, record.chart), 12),
            format!("{}f {}p", folders, record.copies.len() - folders),
            truncate(&record.title, 40),
            record.artist
        );
    }
    println!("{shown} chart(s)");
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        let kept: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{kept}~")
    }
}

/// A chart named by its full id (with or without `sha256:`) or by a unique hex prefix of at least 8 characters.
fn load_manual() -> songs::Manual {
    songs::Manual::load().unwrap_or_else(|e| exit_with(&e))
}

/// Exits when a `--table` value names no installed table, so a typo does not look like an empty result.
fn check_table_filter(filter: &ChartFilter, tables: &TableIndex) {
    let Some(query) = &filter.table else {
        return;
    };
    let known = tables.tables().iter().any(|t| {
        t.name.eq_ignore_ascii_case(query)
            || (!t.symbol.is_empty() && query.starts_with(&t.symbol.to_ascii_lowercase()))
    });
    if !known {
        let dir = env::var("BEETLE_TABLES_DIR").unwrap_or_else(|_| "tables".to_string());
        exit_with(&format!(
            "no installed difficulty table matches '{query}' (tables dir: {dir})"
        ));
    }
}

fn resolve_chart(index: &collection::Index, text: &str) -> ChartId {
    if let Some(id) = ChartId::from_hex(text) {
        if index.locations.iter().any(|l| l.chart == id) {
            return id;
        }
        exit_with(&format!("no scanned chart has the id {}", id.short()));
    }
    let hex = text
        .strip_prefix("sha256:")
        .unwrap_or(text)
        .to_ascii_lowercase();
    if hex.len() < 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        exit_with(&format!(
            "'{text}' is not a chart id (use 8+ hex characters)"
        ));
    }
    let matches: BTreeSet<ChartId> = index
        .locations
        .iter()
        .map(|l| l.chart)
        .filter(|id| id.to_hex().starts_with(&hex))
        .collect();
    match matches.len() {
        1 => matches
            .into_iter()
            .next()
            .unwrap_or_else(|| exit_with(&"unreachable")),
        0 => exit_with(&format!("no chart starts with '{text}'")),
        n => exit_with(&format!("'{text}' matches {n} charts; use more characters")),
    }
}

fn run_song(args: &[String]) {
    match args.first().map(String::as_str) {
        Some("link") => return run_song_link(&args[1..]),
        Some("unlink") => return run_song_unlink(&args[1..]),
        None => exit_with(&"usage: bpm song <song id or title> | link <chart>... | unlink <chart>"),
        _ => {}
    }
    let query = args.join(" ");
    let index = load_index_or_exit();
    let tables = load_tables(&index);
    let collection = songs::build(&index, &load_manual());

    let lowered = query.to_ascii_lowercase();
    let found: Vec<&songs::Song> = collection
        .songs
        .iter()
        .filter(|s| s.id.starts_with(&lowered) || s.title.to_ascii_lowercase() == lowered)
        .collect();
    let song = match found.as_slice() {
        [one] => *one,
        [] => exit_with(&format!("no song matches '{query}'")),
        many => {
            let list: Vec<String> = many
                .iter()
                .map(|s| format!("{} {} / {}", s.id, s.title, s.artist))
                .collect();
            exit_with(&format!(
                "'{query}' matches {} songs; use the id:\n  {}",
                many.len(),
                list.join("\n  ")
            ))
        }
    };

    let copies: usize = song
        .charts
        .iter()
        .map(|&i| collection.records[i].copies.len())
        .sum();
    let packaged = song
        .charts
        .iter()
        .filter(|&&i| collection.records[i].is_packaged())
        .count();
    println!("{} / {}   song:{}", song.title, song.artist, song.id);
    println!(
        "  charts {} · copies {copies} · packaged charts {packaged}",
        song.charts.len()
    );
    for &i in &song.charts {
        let record = &collection.records[i];
        println!();
        println!(
            "  {} {} lv{}  {}  table {}  copies {}",
            normalize_mode(&record.mode),
            record.title,
            record.play_level,
            record.chart.short(),
            table_chips(&tables, record.chart),
            record.copies.len()
        );
        for copy in &record.copies {
            println!("    {}", describe_location(copy));
        }
        if let Some(loaded) = collection::Index::load_index(&record.copies) {
            let copy = record.copies[loaded];
            let note = if copy.is_intact() {
                "first intact copy".to_string()
            } else {
                format!("no intact copy; {} key sound(s) missing", copy.missing_keys)
            };
            println!("    loads: {} ({note})", describe_location(copy));
        }
    }
    if !song.candidates.is_empty() {
        println!();
        println!("  candidates (not merged automatically; join with bpm song link if they are one song):");
        for &c in &song.candidates {
            let other = &collection.songs[c];
            println!(
                "    {} {} / {}  charts {}",
                other.id,
                other.title,
                other.artist,
                other.charts.len()
            );
        }
    }
}

fn run_song_link(args: &[String]) {
    if args.len() < 2 {
        exit_with(&"usage: bpm song link <chart> <chart> [...]");
    }
    let index = load_index_or_exit();
    let group: Vec<ChartId> = args.iter().map(|a| resolve_chart(&index, a)).collect();
    let mut manual = load_manual();

    // Charts that share a song with a group member right now. Linking must not
    // silently push any of them out, so they are reported if it does.
    let before = songs::build(&index, &manual);
    let mates_before: BTreeSet<ChartId> = before
        .songs
        .iter()
        .filter(|s| {
            s.charts
                .iter()
                .any(|&i| group.contains(&before.records[i].chart))
        })
        .flat_map(|s| s.charts.iter().map(|&i| before.records[i].chart))
        .collect();

    // Linking overrides the unlinks between these charts. Splits to charts
    // outside the group stay, so a chart the user separated stays separated.
    manual
        .splits
        .retain(|(a, b)| !(group.contains(a) && group.contains(b)));
    manual.links.push(group.clone());
    manual.save().unwrap_or_else(|e| exit_with(&e));

    let after = songs::build(&index, &manual);
    let song_of = |id: &ChartId| {
        after
            .songs
            .iter()
            .position(|s| s.charts.iter().any(|&i| after.records[i].chart == *id))
    };
    let group_song = song_of(&group[0]);
    let together = group.iter().map(song_of).all(|s| s == group_song);
    if together {
        println!("Linked {} charts into one song.", group.len());
    } else {
        println!(
            "Saved the link, but the {} charts are not one song yet; check with bpm song <id>.",
            group.len()
        );
    }
    let left_out: Vec<String> = mates_before
        .iter()
        .filter(|id| !group.contains(id) && song_of(id) != group_song)
        .map(ChartId::short)
        .collect();
    if !left_out.is_empty() {
        println!(
            "Left out of the song because of earlier unlinks: {}",
            left_out.join(" ")
        );
    }
}
fn run_song_unlink(args: &[String]) {
    if args.len() != 1 {
        exit_with(&"usage: bpm song unlink <chart>");
    }
    let index = load_index_or_exit();
    let chart = resolve_chart(&index, &args[0]);
    let mut manual = load_manual();
    let collection = songs::build(&index, &manual);
    // Everything else in the chart's current song must stop being its song-mate.
    let mates: Vec<ChartId> = collection
        .songs
        .iter()
        .find(|s| {
            s.charts
                .iter()
                .any(|&i| collection.records[i].chart == chart)
        })
        .map(|s| {
            s.charts
                .iter()
                .map(|&i| collection.records[i].chart)
                .filter(|&c| c != chart)
                .collect()
        })
        .unwrap_or_default();

    for group in &mut manual.links {
        group.retain(|id| *id != chart);
    }
    manual.links.retain(|g| g.len() > 1);
    for &mate in &mates {
        let known = manual
            .splits
            .iter()
            .any(|(a, b)| (*a == chart && *b == mate) || (*a == mate && *b == chart));
        if !known {
            manual.splits.push((chart, mate));
        }
    }
    manual.save().unwrap_or_else(|e| exit_with(&e));
    println!(
        "Separated {} from {} other chart(s) in its song.",
        chart.short(),
        mates.len()
    );
}

fn describe_location(copy: &collection::Location) -> String {
    match copy.kind {
        LocationKind::Folder => {
            format!("folder  {}\\{}", copy.source, copy.path.replace('/', "\\"))
        }
        LocationKind::Package => format!("package {} :: {}", copy.source, copy.path),
    }
}

fn run_library_command(args: &[String]) {
    let mut list = load_library();
    let fail = |message: &dyn std::fmt::Display| -> ! {
        eprintln!("Error: {message}");
        std::process::exit(1);
    };
    let save = |list: &beetle_core::LibraryPaths| {
        save_library(list).unwrap_or_else(|e| fail(&e));
    };
    match (args.first().map(String::as_str), args.get(1)) {
        (Some("add"), Some(path)) => {
            let abs = absolute_dir(path).unwrap_or_else(|e| fail(&e));
            if list.add(&abs) {
                save(&list);
                println!("Added {abs} (the player rescans on next start)");
            } else {
                println!("Already registered: {abs}");
            }
        }
        (Some("remove"), Some(path)) => {
            // The folder may be gone already, so fall back to the text as typed.
            let abs = absolute_dir(path).unwrap_or_else(|_| path.clone());
            if list.remove(&abs) {
                save(&list);
                println!("Removed {abs}");
            } else {
                fail(&format!("not registered: {abs}"));
            }
        }
        (Some("list"), _) => {
            if list.paths().is_empty() {
                println!("No folders registered. Add one with: bpm library add <folder>");
            }
            for p in list.paths() {
                let status = if Path::new(p).is_dir() {
                    "ok"
                } else {
                    "missing"
                };
                println!("[{status}] {p}");
            }
        }
        _ => {
            eprintln!("Usage: bpm library <add|remove> <folder> | bpm library list");
            eprintln!("Folders are kept in ./library.dat (or $BEETLE_LIBRARY_FILE), scanned in place by the player.");
            std::process::exit(1);
        }
    }
}

/// Prints `table base`. Nothing is linked here: a confident guess only suggests the command.
fn print_base_report(
    table: &beetle_core::DifficultyTable,
    number: usize,
    report: &table_ops::BaseReport,
    index: &collection::Index,
) {
    let entry = &table.entries[number - 1];
    println!("#{number} {} / {}", entry.title, entry.artist);
    if !report.diff_places.is_empty() {
        println!(
            "  The difference is already in {}; those folders are not candidates.",
            report.diff_places.join(", ")
        );
    }
    if report.diff_copy.is_some() && report.required.is_empty() {
        println!("  The difference already has all its key sounds where it sits, so the original is not needed for sound.");
    }
    let needed = report.required.len();
    let stats = |c: &base_match::Candidate| {
        format!(
            "shares {} of {} sounds it needs ({:.0}% coverage, {:.0}% of the folder's sounds used)",
            c.shared,
            needed,
            c.coverage * 100.0,
            c.precision * 100.0
        )
    };
    let label = |e: base_match::Evidence| match e {
        base_match::Evidence::Strong => "strong",
        base_match::Evidence::Weak => "weak",
        base_match::Evidence::None => "none",
    };
    match &report.verdict {
        base_match::Verdict::Confident(candidate) => {
            println!("  Likely original (high confidence): {}", candidate.place);
            println!("    {}", stats(candidate));
            // The copy of the candidate that shares the most required sounds, so the
            // title shown is the one most likely to be the same song.
            let most_similar = index
                .locations
                .iter()
                .filter(|l| l.is_intact() && base_match::place_id(l) == candidate.place)
                .max_by_key(|l| {
                    l.key_stems
                        .iter()
                        .filter(|stem| report.required.contains(*stem))
                        .count()
                });
            if let Some(rep) = most_similar {
                println!(
                    "    most similar chart there: {} / {}",
                    rep.title, rep.artist
                );
            }
            let songs_now = songs::build(index, &load_manual());
            let grouped = match (report.diff_copy, most_similar) {
                (Some(diff), Some(rep)) => same_song(&songs_now, diff, rep.chart),
                _ => false,
            };
            match (report.diff_copy, most_similar) {
                (Some(diff), Some(rep)) if !grouped => {
                    println!(
                        "  If it is the same song: bpm song link {diff} {}",
                        rep.chart
                    );
                }
                (_, Some(_)) if grouped => {
                    println!("  Already one song in the collection; no link needed.");
                }
                _ => {}
            }
        }
        base_match::Verdict::Ranked(list) => {
            println!("  No confident original. Candidates:");
            for candidate in list {
                println!("    {}: {}", candidate.place, stats(candidate));
            }
        }
        base_match::Verdict::NoKeyMatch(list) => {
            println!("  No folder shares enough key sounds with the difference.");
            for item in list {
                println!(
                    "    skipped for serial sound names; title/artist {}: {}",
                    label(item.evidence),
                    item.place
                );
            }
        }
        base_match::Verdict::MetadataOnly(list) => {
            if report.diff_copy.is_none() {
                println!("  The difference is not in the collection yet, so only title and artist are used:");
            } else {
                println!("  Key sounds cannot decide here (none required, or mostly serial names). Title and artist only:");
            }
            if list.is_empty() {
                println!("    no folder matches the title or artist");
            }
            for item in list {
                println!("    {}: {}", item.place, label(item.evidence));
            }
        }
    }
}

/// Whether two charts already belong to one song in the grouped collection.
fn same_song(collection: &songs::Collection, a: ChartId, b: ChartId) -> bool {
    collection.songs.iter().any(|song| {
        let has = |id: ChartId| {
            song.charts
                .iter()
                .any(|&i| collection.records[i].chart == id)
        };
        has(a) && has(b)
    })
}

fn run_table_command(args: &[String]) {
    let dir = env::var("BEETLE_TABLES_DIR").unwrap_or_else(|_| "tables".to_string());
    let store = TableStore::new(dir);
    let client = HttpClient::with_timeouts(
        std::time::Duration::from_secs(10),
        std::time::Duration::from_secs(30),
    );
    let get = |url: &str, max: u64| client.get_bytes(url, max);
    let fail = |message: &dyn std::fmt::Display| -> ! {
        eprintln!("Error: {message}");
        std::process::exit(1);
    };

    match args.first().map(String::as_str) {
        Some("add") => {
            let Some(address) = args.get(1) else {
                eprintln!("Error: Missing table address.");
                print_table_usage();
                std::process::exit(1);
            };
            println!("Fetching {address} ...");
            let table = fetch_table(&get, address).unwrap_or_else(|e| fail(&e));
            let path = store.install(&table).unwrap_or_else(|e| fail(&e));
            println!(
                "Installed '{}' ({} charts, {} levels) -> {}",
                table.name,
                table.entries.len(),
                table.levels().len(),
                path.display()
            );
        }
        Some("update") => {
            let force = args.iter().any(|a| a == "--force");
            let wanted = args.iter().skip(1).find(|a| !a.starts_with("--"));
            let installed: Vec<_> = match wanted {
                Some(name) => vec![store
                    .find(name)
                    .unwrap_or_else(|| fail(&format!("no installed table named '{name}'")))],
                None => store.list(),
            };
            if installed.is_empty() {
                println!("No tables installed. Add one with `bpm table add <address>`.");
                return;
            }
            let mut failed = false;
            for entry in &installed {
                let name = &entry.1.name;
                match store.update(&get, entry, force, now_secs()) {
                    Ok(UpdateOutcome::Updated {
                        entries,
                        previous_entries,
                    }) => {
                        println!(
                            "{name}: {entries} charts ({:+} since the last update)",
                            entries as i64 - previous_entries as i64
                        );
                    }
                    Ok(UpdateOutcome::TooSoon { minutes_ago }) => {
                        println!("{name}: fetched {minutes_ago} minutes ago, skipped (use --force to fetch anyway)");
                    }
                    Err(e) => {
                        eprintln!("{name}: {e} (the installed copy is unchanged)");
                        failed = true;
                    }
                }
            }
            if failed {
                std::process::exit(1);
            }
        }
        Some("list") => {
            let tables = store.list();
            if tables.is_empty() {
                println!("No tables installed. Add one with `bpm table add <address>`.");
                return;
            }
            let now = now_secs();
            for (path, table) in tables {
                println!(
                    "{:<24} {:<6} {:>6} charts   {:<16} {}",
                    table.name,
                    table.symbol,
                    table.entries.len(),
                    ago(table.fetched, now),
                    path.file_name().and_then(|n| n.to_str()).unwrap_or("")
                );
            }
        }
        Some("remove") => {
            let Some(name) = args.get(1) else {
                eprintln!("Error: Missing table name.");
                print_table_usage();
                std::process::exit(1);
            };
            let removed = store.remove(name).unwrap_or_else(|e| fail(&e));
            println!("Removed '{removed}'.");
        }
        Some("missing") => {
            let wanted = args.iter().skip(1).find(|a| !a.starts_with("--"));
            let tables: Vec<_> = match wanted {
                Some(name) => vec![
                    store
                        .find(name)
                        .unwrap_or_else(|| fail(&format!("no installed table named '{name}'")))
                        .1,
                ],
                None => store.list().into_iter().map(|(_, table)| table).collect(),
            };
            if tables.is_empty() {
                println!("No tables installed. Add one with `bpm table add <address>`.");
                return;
            }
            // Charts in packages count as owned too, so both kinds of copy come from the index.
            let index = load_index_or_exit();
            if index.locations.is_empty() {
                eprintln!("Note: the collection index is empty. Run `bpm scan` first.");
            }
            // A chart counts as owned only with an intact copy (every key sound next to it).
            for table in &tables {
                let rows = table_ops::missing_rows(table, &index);
                let body_needed = rows.iter().filter(|row| row.body_needed).count();
                println!(
                    "{} ({}): {} of {} charts missing ({} need their body)",
                    table.name,
                    table.symbol,
                    rows.len(),
                    table.entries.len(),
                    body_needed
                );
                for row in &rows {
                    let tag = if row.body_needed {
                        "[body needed] "
                    } else {
                        ""
                    };
                    println!(
                        "  #{:<5} {:<6} {tag}{} / {}",
                        row.number, row.level, row.title, row.artist
                    );
                    if !row.url.is_empty() {
                        println!("         {}", row.url);
                    }
                    if !row.url_diff.is_empty() {
                        println!("         diff: {}", row.url_diff);
                    }
                }
                println!();
            }
        }
        Some("fetch") => {
            let Some(name) = args.get(1).filter(|a| !a.starts_with("--")) else {
                eprintln!("Error: Missing table name.");
                print_table_usage();
                std::process::exit(1);
            };
            let yes = args.iter().any(|a| a == "--yes");
            // `--into <folder>` takes the next argument as its value, not as an entry number.
            let into_value = args
                .iter()
                .position(|a| a == "--into")
                .and_then(|i| args.get(i + 1).map(|dir| (i + 1, dir.clone())));
            let into = match &into_value {
                Some((_, dir)) => {
                    if !Path::new(dir).is_dir() {
                        fail(&format!("--into '{dir}' is not an existing folder"));
                    }
                    Some(PathBuf::from(dir))
                }
                None => None,
            };
            let numbers: Vec<usize> = args
                .iter()
                .enumerate()
                .skip(2)
                .filter(|(i, a)| {
                    !a.starts_with("--") && into_value.as_ref().is_none_or(|(v, _)| v != i)
                })
                .map(|(_, a)| a.trim_start_matches('#').parse::<usize>().ok())
                .collect::<Option<_>>()
                .unwrap_or_else(|| {
                    fail(&"entry numbers must be numbers, as `table missing` prints them")
                });
            let (_, table) = store
                .find(name)
                .unwrap_or_else(|| fail(&format!("no installed table named '{name}'")));
            if args.iter().any(|a| a == "--body") {
                open_body_pages(&table, &numbers);
            } else if args.iter().any(|a| a == "--ir") {
                fetch_ir_packs(&client, &table, &numbers, yes, into.as_deref());
            } else {
                fetch_difference_packs(&client, &table, &numbers, yes, into.as_deref());
            }
        }
        Some("base") => {
            let Some(name) = args.get(1) else {
                eprintln!("Error: Missing table name.");
                print_table_usage();
                std::process::exit(1);
            };
            let number = args
                .get(2)
                .and_then(|a| a.trim_start_matches('#').parse::<usize>().ok())
                .unwrap_or_else(|| fail(&"give one entry number, as `table missing` prints it"));
            let (_, table) = store
                .find(name)
                .unwrap_or_else(|| fail(&format!("no installed table named '{name}'")));
            let index = load_index_or_exit();
            let report =
                table_ops::base_report(&table, number, &index).unwrap_or_else(|e| fail(&e));
            print_base_report(&table, number, &report, &index);
        }
        Some("get") => {
            let Some(name) = args.get(1).filter(|a| !a.starts_with("--")) else {
                eprintln!("Error: Missing table name.");
                print_table_usage();
                std::process::exit(1);
            };
            let number = args
                .get(2)
                .and_then(|a| a.trim_start_matches('#').parse::<usize>().ok())
                .unwrap_or_else(|| fail(&"give one entry number, as `table missing` prints it"));
            let yes = args.iter().any(|a| a == "--yes");
            let body_file = args
                .iter()
                .position(|a| a == "--body-file")
                .and_then(|i| args.get(i + 1));
            let (_, table) = store
                .find(name)
                .unwrap_or_else(|| fail(&format!("no installed table named '{name}'")));
            let entry = number
                .checked_sub(1)
                .and_then(|i| table.entries.get(i))
                .unwrap_or_else(|| fail(&format!("no entry #{number} in '{}'", table.name)));
            match body_file {
                None => {
                    if entry.url.is_empty() {
                        println!(
                            "#{number} has no body link in the table. Get the body yourself, then:"
                        );
                    } else {
                        if let Err(e) = table_fetch::open_in_browser(&entry.url) {
                            eprintln!("{e}");
                        }
                        println!("Opened the body page for #{number}. Save the body file, then:");
                    }
                    println!(
                        "  bpm table get \"{}\" {number} --body-file <saved file>",
                        table.name
                    );
                }
                Some(path) => {
                    if let Err(message) =
                        get_entry(&client, &table.name, entry, number, Path::new(path), yes)
                    {
                        eprintln!("#{number}: {message}");
                        std::process::exit(1);
                    }
                }
            }
        }
        _ => print_table_usage(),
    }
}

/// Gets one table entry from a body file: downloads its difference pack when it
/// is a direct link, adds the pack's charts to the body folder beside their key
/// sounds, and imports that folder as a package.
fn get_entry(
    client: &HttpClient,
    table_name: &str,
    entry: &beetle_core::TableEntry,
    number: usize,
    body_file: &Path,
    yes: bool,
) -> Result<(), String> {
    if !body_file.is_file() {
        return Err(format!("'{}' is not a file", body_file.display()));
    }
    let direct_pack = table_fetch::is_direct_pack(&entry.url_diff);
    println!("Plan for #{number} {} / {}:", entry.title, entry.artist);
    println!("  body:    {}", body_file.display());
    if direct_pack {
        println!("  diff:    download {} and check its hash", entry.url_diff);
    }
    println!("  import:  the body folder, as a package, into the package storage");
    if !yes && !confirm("Continue? [y/N] ") {
        println!("Nothing done.");
        return Ok(());
    }

    let scratch = env::temp_dir().join(format!("bpm-get-{}-{number}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let result = get_entry_in(
        client,
        table_name,
        entry,
        number,
        body_file,
        yes,
        &scratch,
        direct_pack,
    );
    let _ = fs::remove_dir_all(&scratch);
    result
}

#[allow(clippy::too_many_arguments)]
fn get_entry_in(
    client: &HttpClient,
    table_name: &str,
    entry: &beetle_core::TableEntry,
    number: usize,
    body_file: &Path,
    yes: bool,
    scratch: &Path,
    direct_pack: bool,
) -> Result<(), String> {
    let body_root = table_ops::unpack_body(body_file, scratch)?;

    if direct_pack {
        let kept = table_ops::fetch_diff(client, entry, &scratch.join("diff"), &body_root)?;
        if kept.matching == 0 {
            return Err(
                "the difference pack has no chart with this entry's hash; nothing imported".into(),
            );
        }
        println!(
            "Added the difference to the body folder ({} file(s) copied, {} already there).",
            kept.copied, kept.skipped
        );
    }

    let Some(chart) = table_ops::find_matching_chart(entry, &body_root) else {
        return Err("the body has no chart with this entry's hash; nothing imported".into());
    };
    let missing = table_fetch::missing_key_sounds(&chart).unwrap_or(0);
    if missing > 0 {
        println!("{missing} key sound(s) of this chart are still missing from the body.");
        if !yes && !confirm("Import it anyway? [y/N] ") {
            return Err("stopped before the import".into());
        }
    }

    let packages = get_default_packages_dir();
    let mut manager = PackageManager::new(&packages).map_err(|e| e.to_string())?;
    let installed = table_ops::import_body_folder(&mut manager, &body_root)?;
    for package in &installed {
        println!(
            "Installed '{}' ({}) -> {}",
            package.name,
            package.id,
            package.location.display()
        );
    }
    println!("Imported #{number} from '{table_name}' into the package storage.");
    Ok(())
}

/// Opens the body page (the entry's `url`) of each chosen entry in the browser.
/// Nothing is downloaded: the user saves the body and runs `bpm scan`.
fn open_body_pages(table: &beetle_core::DifficultyTable, numbers: &[usize]) {
    for &number in numbers {
        let Some(entry) = number.checked_sub(1).and_then(|i| table.entries.get(i)) else {
            eprintln!("#{number}: no such entry in '{}'", table.name);
            continue;
        };
        if entry.url.is_empty() {
            println!("#{number}: no body link in the table");
            continue;
        }
        match table_fetch::open_in_browser(&entry.url) {
            Ok(()) => println!("#{number}: opened {}", entry.url),
            Err(e) => eprintln!("#{number}: {e}"),
        }
    }
}

/// Downloads the difference packs of the chosen table entries (numbers as
/// `table missing` prints them), after the user confirms. Each pack goes to
/// `songs/<table>/<number>` when one of its charts has the entry's hash.
fn fetch_difference_packs(
    client: &HttpClient,
    table: &beetle_core::DifficultyTable,
    numbers: &[usize],
    yes: bool,
    into: Option<&Path>,
) {
    let mut planned = Vec::new();
    for &number in numbers {
        let Some(entry) = number.checked_sub(1).and_then(|i| table.entries.get(i)) else {
            eprintln!("#{number}: no such entry in '{}'", table.name);
            continue;
        };
        if table_fetch::is_direct_pack(&entry.url_diff) {
            planned.push((number, entry, vec![entry.url_diff.clone()]));
        } else if entry.url_diff.is_empty() {
            println!("#{number}: no difference link in the table");
        } else {
            println!(
                "#{number}: not a direct pack link. Open it in a browser: {}",
                entry.url_diff
            );
        }
    }
    if planned.is_empty() {
        return;
    }

    println!(
        "About to download {} pack(s) for '{}':",
        planned.len(),
        table.name
    );
    for (number, entry, urls) in &planned {
        println!(
            "  #{number} {} / {}  {}",
            entry.title, entry.artist, urls[0]
        );
    }
    if !yes && !confirm("Download these packs? [y/N] ") {
        println!("Nothing downloaded.");
        return;
    }

    let slug = TableStore::slug(&table.name);
    for (number, entry, urls) in planned {
        for url in urls {
            report_download(client, number, entry, &url, &slug, into);
        }
    }
}

/// One link that a Stella IR chart page lists for a table entry: `body` or `diff`
/// (the row it came from), and its address.
struct IrLink {
    label: &'static str,
    url: String,
}

/// Looks up the chosen table entries on the Stella IR chart page of their MD5 and
/// lists the body and diff links that page gives. After the user confirms, each
/// link is downloaded. A pack is kept only when one of its charts has the
/// entry's hash, the same rule as for the direct packs. Nothing is guessed from
/// the host: every link comes from the page and is shown before it is fetched.
fn fetch_ir_packs(
    client: &HttpClient,
    table: &beetle_core::DifficultyTable,
    numbers: &[usize],
    yes: bool,
    into: Option<&Path>,
) {
    let mut planned: Vec<(usize, &beetle_core::TableEntry, Vec<IrLink>)> = Vec::new();
    for &number in numbers {
        let Some(entry) = number.checked_sub(1).and_then(|i| table.entries.get(i)) else {
            eprintln!("#{number}: no such entry in '{}'", table.name);
            continue;
        };
        let Some(md5) = entry.md5 else {
            println!("#{number}: the table gives no MD5, so the IR page cannot be looked up");
            continue;
        };
        let links = match ir::fetch_chart_links(client, &md5) {
            Ok(links) => links,
            Err(e) => {
                eprintln!("#{number}: {e}");
                continue;
            }
        };
        let mut urls = Vec::new();
        for (label, found) in [("body", links.body), ("diff", links.diff)] {
            for url in found {
                if table_fetch::pack_extension(&url).is_some() {
                    urls.push(IrLink { label, url });
                } else {
                    println!("#{number}: not an archive link. Open it in a browser: {url}");
                }
            }
        }
        if urls.is_empty() {
            println!("#{number}: the IR page lists no archive link");
        } else {
            planned.push((number, entry, urls));
        }
    }
    if planned.is_empty() {
        return;
    }

    println!(
        "About to download {} link(s) for '{}' from the IR pages:",
        planned.iter().map(|(_, _, urls)| urls.len()).sum::<usize>(),
        table.name
    );
    for (number, entry, urls) in &planned {
        for link in urls {
            println!(
                "  #{number} {} / {}  {}: {}",
                entry.title, entry.artist, link.label, link.url
            );
        }
    }
    if !yes && !confirm("Download these links? [y/N] ") {
        println!("Nothing downloaded.");
        return;
    }

    let slug = TableStore::slug(&table.name);
    for (number, entry, urls) in planned {
        for link in urls {
            println!("#{number} {}:", link.label);
            report_download(client, number, entry, &link.url, &slug, into);
        }
    }
}

/// Downloads one pack, keeps its matching charts, and prints the result.
fn report_download(
    client: &HttpClient,
    number: usize,
    entry: &beetle_core::TableEntry,
    url: &str,
    slug: &str,
    into: Option<&Path>,
) {
    let scratch = env::temp_dir().join(format!("bpm-fetch-{}-{number}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    if let Err(e) = fs::create_dir_all(&scratch) {
        eprintln!("#{number}: {e}");
        return;
    }
    let result = download_pack(client, url, entry, number, &scratch, slug, into);
    let _ = fs::remove_dir_all(&scratch);
    match result {
        Ok((kept, pack_dir)) if kept.matching > 0 => println!(
            "#{number}: kept {} matching chart(s) in {}: {} file(s) copied, {} already there and left alone. Run `bpm scan` to index them.\n         It counts as owned only once its key sounds are in the collection. Body page: {}",
            kept.matching,
            pack_dir.display(),
            kept.copied,
            kept.skipped,
            entry.url
        ),
        Ok(_) => {
            eprintln!("#{number}: no chart in the pack matches the table entry; nothing kept")
        }
        Err(e) => eprintln!("#{number}: {e}"),
    }
}

/// Downloads `url` into `scratch` and keeps the pack when one of its charts has
/// the entry's hash. The archive type comes from the link, and is zip when the
/// link names none (the Satellite upload links have no extension).
fn download_pack(
    client: &HttpClient,
    url: &str,
    entry: &beetle_core::TableEntry,
    number: usize,
    scratch: &Path,
    slug: &str,
    into: Option<&Path>,
) -> Result<(table_fetch::KeptPack, PathBuf), String> {
    let ext = table_fetch::pack_extension(url).unwrap_or("zip");
    let bytes = client
        .get_bytes(url, table_fetch::MAX_PACK_BYTES)
        .map_err(|e| format!("cannot download {url}: {e}"))?;
    let archive = scratch.join(format!("pack.{ext}"));
    fs::write(&archive, bytes).map_err(|e| e.to_string())?;
    let pack_dir = match into {
        Some(dir) => dir.to_path_buf(),
        None => PathBuf::from("songs").join(slug).join(number.to_string()),
    };
    table_fetch::keep_pack(&archive, entry, scratch, &pack_dir).map(|kept| (kept, pack_dir))
}

/// Asks a yes/no question on stdin. Anything but `y` or `yes` is no.
fn confirm(question: &str) -> bool {
    print!("{question}");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Returns true when `path` names a zip archive by its extension.
fn is_zip_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// Imports every BMS song folder found under `folder` into managed storage.
///
/// `display_name` is the user-supplied path, used only in messages.
/// A single song is reported individually; several songs are imported one by one and
/// failures are reported without stopping the batch.
fn import_bms_folders(
    manager: &mut PackageManager,
    folder: &str,
    display_name: &str,
) -> Result<(), String> {
    let roots = bms_package_manager::find_bms_song_roots(folder);
    if roots.is_empty() {
        return Err(format!(
            "Error: No BMS chart files found in '{display_name}'"
        ));
    }

    if roots.len() == 1 {
        let target_root = &roots[0];
        if target_root != Path::new(folder) {
            println!("Detected BMS song root at '{}'", target_root.display());
        }
        let installed = manager
            .import_folder(target_root, None)
            .map_err(|e| format!("Import failed: {e}"))?;
        println!(
            "Successfully imported and installed '{}' ({}) -> state {}",
            installed.name, installed.id, installed.state_hash
        );
        println!("Location: {}", installed.location.display());
    } else {
        println!(
            "Found {} BMS song directories under '{}'. Batch importing each...",
            roots.len(),
            display_name
        );
        let mut success = 0;
        for (i, target_root) in roots.iter().enumerate() {
            print!(
                "[{}/{}] Importing '{}'... ",
                i + 1,
                roots.len(),
                target_root.display()
            );
            match manager.import_folder(target_root, None) {
                Ok(installed) => {
                    println!("OK -> '{}' ({})", installed.name, installed.id);
                    success += 1;
                }
                Err(e) => {
                    println!("FAILED ({e})");
                }
            }
        }
        println!(
            "Batch import finished: {}/{} songs imported into registry.",
            success,
            roots.len()
        );
    }
    Ok(())
}

fn main() -> Result<(), PackageManagerError> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    if args[1] == "library" {
        run_library_command(&args[2..]);
        return Ok(());
    }

    // Difficulty tables have nothing to do with package storage.
    if args[1] == "table" {
        run_table_command(&args[2..]);
        return Ok(());
    }

    let storage_dir = get_default_packages_dir();
    let mut manager = PackageManager::new(&storage_dir)?;

    match args[1].as_str() {
        "pack" => {
            if args.len() < 3 {
                eprintln!("Error: Missing folder path.");
                eprintln!(
                    "Usage: bpm pack <folder_path> [-o <output.bmsp>] [--base <base.bmsp_or_dir>]"
                );
                std::process::exit(1);
            }
            let folder = &args[2];

            // Check if --base was passed to create a delta directly
            let base_idx = args.iter().position(|a| a == "--base");
            if let Some(idx) = base_idx {
                if idx + 1 >= args.len() {
                    eprintln!("Error: Missing base path after --base.");
                    std::process::exit(1);
                }
                let base_path = &args[idx + 1];
                let out_file = if let Some(o_idx) = args.iter().position(|a| a == "-o") {
                    if o_idx + 1 < args.len() {
                        args[o_idx + 1].clone()
                    } else {
                        "delta.bmdp".to_string()
                    }
                } else {
                    let folder_name = PathBuf::from(folder)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("delta")
                        .to_string();
                    format!("{}.bmdp", folder_name)
                };

                match PackageUpdater::create_delta_between_paths(base_path, folder) {
                    Ok(bytes) => {
                        if let Err(e) = fs::write(&out_file, bytes) {
                            eprintln!("Failed to write output delta file: {e}");
                            std::process::exit(1);
                        }
                        println!(
                            "Successfully generated delta '{}' based on '{}'",
                            out_file, base_path
                        );
                    }
                    Err(e) => {
                        eprintln!("Delta creation failed: {e}");
                        std::process::exit(1);
                    }
                }
                return Ok(());
            }

            let is_turbo = args.iter().any(|a| a == "--atlas" || a == "--turbo")
                || args
                    .windows(2)
                    .any(|w| w[0] == "--profile" && w[1] == "turbo");
            let profile = if is_turbo {
                bms_package_manager::PackProfile::Turbo
            } else {
                bms_package_manager::PackProfile::Classic
            };

            let split_bga = args.iter().any(|a| a == "--split-bga");
            let no_video = args.iter().any(|a| a == "--no-video");
            let bga_mode = if split_bga {
                bms_package_manager::BgaPackMode::Split
            } else if no_video {
                bms_package_manager::BgaPackMode::NoVideo
            } else {
                bms_package_manager::BgaPackMode::Embed
            };

            let flac_mode = args.iter().any(|a| a == "--flac");
            let audio_mode = if flac_mode {
                bms_package_manager::AudioPackMode::Flac
            } else {
                bms_package_manager::AudioPackMode::Auto
            };
            let pack_options = bms_package_manager::PackOptions::new(profile, bga_mode)
                .with_audio_mode(audio_mode);

            let roots = bms_package_manager::find_bms_song_roots(folder);
            if roots.is_empty() {
                eprintln!("Error: No BMS chart files (.bms, .bme, .bml, .pms) found in '{}' or any subdirectories.", folder);
                std::process::exit(1);
            }

            let profile_tag = match profile {
                bms_package_manager::PackProfile::Classic => "Classic",
                bms_package_manager::PackProfile::Turbo => "Turbo (Dual Atlas)",
            };

            let out_idx = args.iter().position(|a| a == "-o");
            let out_arg = out_idx.and_then(|idx| args.get(idx + 1));

            if roots.len() == 1 {
                let target_root = &roots[0];
                let folder_name = target_root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("package")
                    .to_string();

                let out_file: PathBuf = if let Some(dest) = out_arg {
                    let dest_path = Path::new(dest);
                    if dest_path.is_dir() || dest.ends_with('/') || dest.ends_with('\\') {
                        let _ = fs::create_dir_all(dest_path);
                        dest_path.join(format!("{}.bmsp", folder_name))
                    } else {
                        PathBuf::from(dest)
                    }
                } else {
                    PathBuf::from(format!("{}.bmsp", folder_name))
                };

                if target_root != std::path::Path::new(folder) {
                    println!("Detected BMS song root at '{}'", target_root.display());
                }

                match bms_package_manager::pack_bms_folder_advanced_with_progress(
                    target_root,
                    None,
                    pack_options,
                    None,
                    |_, _, _, _| {},
                ) {
                    Ok(pack_out) => {
                        if let Err(e) = fs::write(&out_file, &pack_out.base_package) {
                            eprintln!("Failed to write output package file: {e}");
                            std::process::exit(1);
                        }
                        println!(
                            "Successfully packed '{}' into '{}' [{}]",
                            target_root.display(),
                            out_file.display(),
                            profile_tag
                        );
                        if let Some(bga_bytes) = pack_out.bga_package {
                            let companion_file = out_file.with_extension("bga.bmsp");
                            if let Err(e) = fs::write(&companion_file, &bga_bytes) {
                                eprintln!("Failed to write BGA companion file: {e}");
                                std::process::exit(1);
                            }
                            println!(
                                "Companion BGA package written to '{}' ({} bytes)",
                                companion_file.display(),
                                bga_bytes.len()
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Packaging failed: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                println!(
                    "Found {} BMS song directories under '{}'. Batch packing each song...",
                    roots.len(),
                    folder
                );
                let out_dir: Option<PathBuf> = out_arg.map(|dest| {
                    let p = PathBuf::from(dest);
                    let _ = fs::create_dir_all(&p);
                    p
                });

                let mut success_count = 0;
                for (i, target_root) in roots.iter().enumerate() {
                    let folder_name = target_root
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("song")
                        .to_string();
                    let out_file = match &out_dir {
                        Some(dir) => dir.join(format!("{}.bmsp", folder_name)),
                        None => {
                            let parent = Path::new(folder);
                            if parent.is_dir() {
                                parent.join(format!("{}.bmsp", folder_name))
                            } else {
                                PathBuf::from(format!("{}.bmsp", folder_name))
                            }
                        }
                    };

                    print!(
                        "[{}/{}] Packing '{}' into '{}' [{}]... ",
                        i + 1,
                        roots.len(),
                        target_root.display(),
                        out_file.display(),
                        profile_tag
                    );
                    match bms_package_manager::pack_bms_folder_advanced_with_progress(
                        target_root,
                        None,
                        pack_options,
                        None,
                        |_, _, _, _| {},
                    ) {
                        Ok(pack_out) => {
                            if let Err(e) = fs::write(&out_file, &pack_out.base_package) {
                                println!("FAILED (Write error: {})", e);
                            } else {
                                if let Some(bga_bytes) = pack_out.bga_package {
                                    let companion_file = out_file.with_extension("bga.bmsp");
                                    let _ = fs::write(&companion_file, &bga_bytes);
                                    println!(
                                        "OK ({} bytes + {} bytes BGA)",
                                        pack_out.base_package.len(),
                                        bga_bytes.len()
                                    );
                                } else {
                                    println!("OK ({} bytes)", pack_out.base_package.len());
                                }
                                success_count += 1;
                            }
                        }
                        Err(e) => {
                            println!("FAILED ({})", e);
                        }
                    }
                }
                println!(
                    "Batch packing finished: {}/{} packages created successfully.",
                    success_count,
                    roots.len()
                );
            }
        }
        "diff" => {
            if args.len() < 4 {
                eprintln!("Error: Missing arguments.");
                eprintln!(
                    "Usage: bpm diff <base_path_or_bmsp> <target_path_or_bmsp> [-o <diff.bmdp>]"
                );
                std::process::exit(1);
            }
            let base_path = &args[2];
            let target_path = &args[3];
            let out_file = if args.len() >= 6 && args[4] == "-o" {
                args[5].clone()
            } else {
                "update.bmdp".to_string()
            };

            match PackageUpdater::create_delta_between_paths(base_path, target_path) {
                Ok(delta_bytes) => {
                    if let Err(e) = fs::write(&out_file, delta_bytes) {
                        eprintln!("Failed to write output delta file: {e}");
                        std::process::exit(1);
                    }
                    println!(
                        "Successfully generated delta package '{}' (Base: '{}' -> Target: '{}')",
                        out_file, base_path, target_path
                    );
                }
                Err(e) => {
                    eprintln!("Diff generation failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        "patch" => {
            if args.len() < 4 {
                eprintln!("Error: Missing arguments.");
                eprintln!("Usage: bpm patch <base.bmsp> <diff.bmdp> [-o <out_target.bmsp>]");
                std::process::exit(1);
            }
            let base_path = &args[2];
            let diff_path = &args[3];
            let out_file = if args.len() >= 6 && args[4] == "-o" {
                args[5].clone()
            } else {
                "patched_target.bmsp".to_string()
            };

            let base_pkg = bms_package::Package::open(base_path)?;
            let mut delta_pkg = bms_package::DeltaPackage::open_file(diff_path)?;
            let base_raw_bytes = fs::read(base_path).ok();
            let target_bytes = bms_package::DeltaApplicator::apply_to_bytes(
                &base_pkg,
                &mut delta_pkg,
                base_raw_bytes.as_deref(),
            )?;

            if let Err(e) = fs::write(&out_file, target_bytes) {
                eprintln!("Failed to write patched package file: {e}");
                std::process::exit(1);
            }
            println!(
                "Successfully reconstructed target package '{}' from base '{}' and diff '{}'",
                out_file, base_path, diff_path
            );
        }
        "update" => {
            // Check if user passed a .bmdp delta file for backward compatibility
            if args.len() >= 3 && (args[2].ends_with(".bmdp") || Path::new(&args[2]).is_file()) {
                let delta_path = &args[2];
                match manager.apply_delta(delta_path) {
                    Ok(installed) => {
                        println!(
                            "Successfully applied delta and updated '{}' ({}) -> state {}",
                            installed.name, installed.id, installed.state_hash
                        );
                        println!("Location: {}", installed.location.display());
                    }
                    Err(e) => {
                        eprintln!("Delta update failed: {e}");
                        std::process::exit(1);
                    }
                }
                return Ok(());
            }

            // Remote registry index update
            let sources_path = storage_dir.join("sources.json");
            let sources_cfg = SourcesConfig::load_or_init(&sources_path)
                .map_err(PackageManagerError::StorageError)?;
            let cache_mgr = RegistryCacheManager::new(&storage_dir);
            let client = HttpClient::default();

            println!("Updating remote registry sources...");
            let active_sources = sources_cfg.active_sources_by_priority();
            if active_sources.is_empty() {
                println!("No active sources configured. Use 'bpm source add' to add a source.");
                return Ok(());
            }

            let mut total_packages = 0;
            let mut updated_count = 0;
            for source in active_sources {
                print!("  [{}] {} ... ", source.id, source.url);
                use std::io::Write;
                let _ = std::io::stdout().flush();
                match cache_mgr.update_or_fallback(&client, source) {
                    Ok((index, from_cache)) => {
                        let count = index.packages.len();
                        total_packages += count;
                        if from_cache {
                            println!("OFFLINE (loaded {} packages from cache)", count);
                        } else {
                            println!("OK ({} packages indexed)", count);
                            updated_count += 1;
                        }
                    }
                    Err(e) => {
                        println!("FAILED ({e})");
                    }
                }
            }
            println!(
                "Registry update complete: {}/{} sources updated. Total available remote packages: {}",
                updated_count,
                sources_cfg.sources.len(),
                total_packages
            );
        }
        "search" => {
            let query = args.get(2).map(|s| s.trim()).unwrap_or("");
            let sources_path = storage_dir.join("sources.json");
            let sources_cfg = SourcesConfig::load_or_init(&sources_path)
                .map_err(PackageManagerError::StorageError)?;
            let cache_mgr = RegistryCacheManager::new(&storage_dir);

            let active_sources = sources_cfg.active_sources_by_priority();
            let cached_indexes = cache_mgr.load_all_cached(&active_sources);
            let pairs: Vec<(&RegistrySource, &RemoteRegistryIndex)> =
                cached_indexes.iter().map(|(s, idx)| (s, idx)).collect();
            let mut packages = SourcesConfig::merge_packages(&pairs);

            if !query.is_empty() {
                let q_lower = query.to_lowercase();
                packages.retain(|p| {
                    p.id.to_lowercase().contains(&q_lower)
                        || p.title.to_lowercase().contains(&q_lower)
                        || p.artist.to_lowercase().contains(&q_lower)
                        || p.genre
                            .as_deref()
                            .map(|g| g.to_lowercase().contains(&q_lower))
                            .unwrap_or(false)
                });
            }

            if packages.is_empty() {
                if query.is_empty() {
                    println!(
                        "No packages found in registry cache. Run 'bpm update' to fetch indexes."
                    );
                } else {
                    println!("No remote packages matching '{query}' found.");
                }
                return Ok(());
            }

            println!(
                "{:<20} {:<24} {:<16} {:<12} {:<10} STATUS",
                "ID", "TITLE", "ARTIST", "GENRE", "SIZE"
            );
            println!("{:-<95}", "");

            for pkg in packages {
                let artist = if pkg.artist.len() > 14 {
                    format!("{}...", &pkg.artist[..12])
                } else {
                    pkg.artist.clone()
                };
                let genre = pkg.genre.as_deref().unwrap_or("-");
                let size_mb = pkg.size_bytes as f64 / (1024.0 * 1024.0);
                let size_str = format!("{:.1} MB", size_mb);

                let status = match manager.get_package(&pkg.id) {
                    Some(installed) => {
                        if installed.active_state == pkg.state_hash {
                            "[Installed]"
                        } else {
                            "[Update Available]"
                        }
                    }
                    None => "[Available]",
                };

                let title = if pkg.title.len() > 22 {
                    format!("{}...", &pkg.title[..20])
                } else {
                    pkg.title.clone()
                };

                println!(
                    "{:<20} {:<24} {:<16} {:<12} {:<10} {}",
                    pkg.id, title, artist, genre, size_str, status
                );
            }
        }
        "import" => {
            if args.len() < 3 {
                eprintln!("Error: Missing folder path or zip archive.");
                eprintln!("Usage: bpm import <folder_path_or_zip>");
                std::process::exit(1);
            }
            let input = &args[2];
            if is_zip_path(input) {
                let extract_dir = env::temp_dir().join(format!(
                    "bpm_import_{}_{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                ));
                println!("Extracting '{}'...", input);
                let result = match bms_package_manager::extract_zip_archive(input, &extract_dir) {
                    Ok(count) => {
                        println!("Extracted {} files.", count);
                        import_bms_folders(&mut manager, &extract_dir.to_string_lossy(), input)
                    }
                    Err(e) => Err(format!("Failed to extract '{input}': {e}")),
                };
                let _ = fs::remove_dir_all(&extract_dir);
                if let Err(msg) = result {
                    eprintln!("{msg}");
                    std::process::exit(1);
                }
            } else if let Err(msg) = import_bms_folders(&mut manager, input, input) {
                eprintln!("{msg}");
                std::process::exit(1);
            }
        }
        "install" => {
            let with_bga = args.iter().any(|a| a == "--with-bga");
            let path_arg = args.iter().skip(2).find(|a| *a != "--with-bga");
            let Some(path) = path_arg else {
                eprintln!("Error: Missing package file path or remote package ID.");
                eprintln!("Usage: bpm install <package.bmsp_or_id> [--with-bga]");
                std::process::exit(1);
            };

            let is_local_file =
                Path::new(path).is_file() || (path.ends_with(".bmsp") && Path::new(path).exists());

            if is_local_file {
                match manager.install(path) {
                    Ok(installed) => {
                        println!(
                            "Successfully installed '{}' ({}) -> state {}",
                            installed.name, installed.id, installed.state_hash
                        );
                        println!("Location: {}", installed.location.display());

                        // Check for companion package if --with-bga is passed or if adjacent
                        let base_path = Path::new(path);
                        let file_name =
                            base_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        let companion_name = if file_name.ends_with(".bmsp") {
                            format!("{}.bga.bmsp", file_name.trim_end_matches(".bmsp"))
                        } else {
                            format!("{}.bga.bmsp", file_name)
                        };
                        let candidate = base_path
                            .parent()
                            .map(|p| p.join(&companion_name))
                            .unwrap_or_else(|| PathBuf::from(&companion_name));

                        if with_bga {
                            if candidate.exists() {
                                match manager.install_bga_companion(&candidate) {
                                    Ok(t_id) => {
                                        println!(
                                            "Installed companion BGA package for '{}' from '{}'",
                                            t_id,
                                            candidate.display()
                                        );
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "Failed to install BGA companion '{}': {e}",
                                            candidate.display()
                                        );
                                    }
                                }
                            } else {
                                eprintln!("Warning: --with-bga was specified, but companion package '{}' was not found.", candidate.display());
                            }
                        } else if candidate.exists() {
                            println!(
                                "Notice: Decoupled BGA companion '{}' is available.",
                                candidate.display()
                            );
                            println!(
                                "        Install it using: bpm bga install \"{}\"",
                                candidate.display()
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Installation failed: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                // Remote package download and installation
                let sources_path = storage_dir.join("sources.json");
                let sources_cfg = SourcesConfig::load_or_init(&sources_path)
                    .map_err(PackageManagerError::StorageError)?;
                let cache_mgr = RegistryCacheManager::new(&storage_dir);
                let active_sources = sources_cfg.active_sources_by_priority();
                let mut cached_indexes = cache_mgr.load_all_cached(&active_sources);

                // If no packages cached, auto-update sources
                if cached_indexes.is_empty() {
                    println!("No cached indexes found. Fetching from remote sources...");
                    let client = HttpClient::default();
                    for src in &active_sources {
                        let _ = cache_mgr.update_or_fallback(&client, src);
                    }
                    cached_indexes = cache_mgr.load_all_cached(&active_sources);
                }

                let pairs: Vec<(&RegistrySource, &RemoteRegistryIndex)> =
                    cached_indexes.iter().map(|(s, idx)| (s, idx)).collect();
                let packages = SourcesConfig::merge_packages(&pairs);

                let Some(target_pkg) = packages.iter().find(|p| p.id.eq_ignore_ascii_case(path))
                else {
                    eprintln!(
                        "Error: Package '{}' not found as a local file or in remote registries.",
                        path
                    );
                    eprintln!("       Run 'bpm update' to refresh remote package lists or 'bpm search' to browse available songs.");
                    std::process::exit(1);
                };

                let size_mb = target_pkg.size_bytes as f64 / (1024.0 * 1024.0);
                println!(
                    "Found remote package '{}' ({}) version {} [{:.2} MB]",
                    target_pkg.title, target_pkg.id, target_pkg.version, size_mb
                );
                println!("Downloading from: {}", target_pkg.download_url);

                let installer = RemotePackageInstaller::new(&storage_dir);
                let client = HttpClient::default();

                let _installed = match installer.install_remote_package(
                    &mut manager,
                    &client,
                    target_pkg,
                    "",
                    |cur, tot| {
                        print_progress_bar("  ", cur, tot);
                    },
                ) {
                    Ok(pkg) => {
                        println!();
                        println!(
                            "Successfully installed '{}' ({}) -> state {}",
                            pkg.name, pkg.id, pkg.state_hash
                        );
                        println!("Location: {}", pkg.location.display());
                        pkg
                    }
                    Err(e) => {
                        println!();
                        eprintln!("Installation failed: {e}");
                        std::process::exit(1);
                    }
                };

                // Check companion BGA
                if let Some(ref bga_meta) = target_pkg.companion_bga {
                    let bga_size_mb = bga_meta.size_bytes as f64 / (1024.0 * 1024.0);
                    if with_bga {
                        println!(
                            "Downloading companion BGA package '{}' [{:.2} MB]...",
                            bga_meta.id, bga_size_mb
                        );
                        match installer.install_remote_bga_companion(
                            &mut manager,
                            &client,
                            bga_meta,
                            "",
                            |cur, tot| {
                                print_progress_bar("  [BGA] ", cur, tot);
                            },
                        ) {
                            Ok(tid) => {
                                println!();
                                println!("Installed companion BGA package for '{}'", tid);
                            }
                            Err(e) => {
                                println!();
                                eprintln!("Failed to install BGA companion: {e}");
                            }
                        }
                    } else {
                        println!(
                            "Notice: Decoupled BGA companion ({:.2} MB) is available.",
                            bga_size_mb
                        );
                        println!(
                            "        Install it using: bpm bga install \"{}\" or bpm install \"{}\" --with-bga",
                            target_pkg.id, target_pkg.id
                        );
                    }
                }
            }
        }
        "bga" => {
            if args.len() < 3 {
                eprintln!("Error: Missing BGA subcommand.");
                eprintln!("Usage: bpm bga <install|remove|status> <args...>");
                std::process::exit(1);
            }
            match args[2].as_str() {
                "install" => {
                    if args.len() < 4 {
                        eprintln!("Error: Missing companion package path or package ID.");
                        eprintln!("Usage: bpm bga install <package.bga.bmsp_or_id>");
                        std::process::exit(1);
                    }
                    let bga_arg = &args[3];
                    if Path::new(bga_arg).is_file() {
                        match manager.install_bga_companion(bga_arg) {
                            Ok(target_id) => {
                                println!(
                                    "Successfully installed BGA companion for package '{}' from '{}'",
                                    target_id, bga_arg
                                );
                            }
                            Err(e) => {
                                eprintln!("Failed to install BGA companion: {e}");
                                std::process::exit(1);
                            }
                        }
                    } else {
                        // Remote BGA install
                        let sources_path = storage_dir.join("sources.json");
                        let sources_cfg = SourcesConfig::load_or_init(&sources_path)
                            .map_err(PackageManagerError::StorageError)?;
                        let cache_mgr = RegistryCacheManager::new(&storage_dir);
                        let active_sources = sources_cfg.active_sources_by_priority();
                        let cached_indexes = cache_mgr.load_all_cached(&active_sources);
                        let pairs: Vec<(&RegistrySource, &RemoteRegistryIndex)> =
                            cached_indexes.iter().map(|(s, idx)| (s, idx)).collect();
                        let packages = SourcesConfig::merge_packages(&pairs);
                        let target_pkg =
                            packages.iter().find(|p| p.id.eq_ignore_ascii_case(bga_arg));
                        if let Some(pkg) = target_pkg {
                            if let Some(ref bga_meta) = pkg.companion_bga {
                                let installer = RemotePackageInstaller::new(&storage_dir);
                                let client = HttpClient::default();
                                println!("Downloading companion BGA for '{}'...", pkg.id);
                                match installer.install_remote_bga_companion(
                                    &mut manager,
                                    &client,
                                    bga_meta,
                                    "",
                                    |cur, tot| {
                                        print_progress_bar("  [BGA] ", cur, tot);
                                    },
                                ) {
                                    Ok(tid) => {
                                        println!();
                                        println!(
                                            "Successfully installed companion BGA for '{}'",
                                            tid
                                        );
                                    }
                                    Err(e) => {
                                        println!();
                                        eprintln!("Failed to install remote BGA companion: {e}");
                                        std::process::exit(1);
                                    }
                                }
                            } else {
                                eprintln!(
                                    "Package '{}' does not have a companion BGA in remote registry.",
                                    bga_arg
                                );
                                std::process::exit(1);
                            }
                        } else {
                            eprintln!("Package or file '{}' not found.", bga_arg);
                            std::process::exit(1);
                        }
                    }
                }
                "remove" => {
                    if args.len() < 4 {
                        eprintln!("Error: Missing package ID.");
                        eprintln!("Usage: bpm bga remove <package_id>");
                        std::process::exit(1);
                    }
                    let target_id = &args[3];
                    match manager.remove_bga_companion(target_id) {
                        Ok(reclaimed) => {
                            let mb = reclaimed as f64 / (1024.0 * 1024.0);
                            println!(
                                "Successfully removed BGA companion for '{}' (reclaimed {:.2} MB / {} bytes)",
                                target_id, mb, reclaimed
                            );
                        }
                        Err(e) => {
                            eprintln!("Failed to remove BGA companion: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                "status" => {
                    if args.len() < 4 {
                        eprintln!("Error: Missing package ID.");
                        eprintln!("Usage: bpm bga status <package_id>");
                        std::process::exit(1);
                    }
                    let target_id = &args[3];
                    match manager.get_package(target_id) {
                        Some(pkg) => {
                            println!("Package ID:    {}", pkg.id);
                            println!("BGA Status:    {}", pkg.bga_status.as_str());
                            if let Some(ref path) = pkg.bga_companion_path {
                                println!("Companion:     {}", path);
                            }
                        }
                        None => {
                            eprintln!("Package '{}' not found.", target_id);
                            std::process::exit(1);
                        }
                    }
                }
                other => {
                    eprintln!("Unknown bga command: '{other}'");
                    eprintln!("Usage: bpm bga <install|remove|status>");
                    std::process::exit(1);
                }
            }
        }
        "scan" => run_scan(&manager),
        "status" => run_status(),
        "dupes" => run_dupes(&args[2..]),
        "songs" => run_songs(&args[2..]),
        "song" => run_song(&args[2..]),
        "charts" => run_charts(&args[2..]),
        "list" => {
            let packages = manager.list_active_packages();
            if packages.is_empty() {
                println!("No packages installed.");
                return Ok(());
            }

            println!(
                "{:<25} {:<16} {:<12} {:<30} AUTHOR",
                "ID", "STATE", "BGA", "NAME"
            );
            println!("{:-<95}", "");
            for pkg in packages {
                let author = pkg.author.as_deref().unwrap_or("-");
                let short_hash = if pkg.state_hash.len() > 12 {
                    &pkg.state_hash[..12]
                } else {
                    &pkg.state_hash
                };
                println!(
                    "{:<25} {:<16} {:<12} {:<30} {}",
                    pkg.id,
                    short_hash,
                    pkg.bga_status.as_str(),
                    pkg.name,
                    author
                );
            }
        }
        "info" => {
            if args.len() < 3 {
                eprintln!("Error: Missing package ID.");
                eprintln!("Usage: bpm info <package_id>");
                std::process::exit(1);
            }
            let id = &args[2];
            match manager.get_package(id) {
                Some(record) => {
                    println!("Package ID:      {}", record.id);
                    println!("Name:            {}", record.name);
                    println!(
                        "Author:          {}",
                        record.author.as_deref().unwrap_or("-")
                    );
                    println!("Active State:    {}", record.active_state);
                    println!("BGA Status:      {}", record.bga_status.as_str());
                    if let Some(ref path) = record.bga_companion_path {
                        println!("BGA Companion:   {}", path);
                    }
                    println!("Installed States:");
                    for (state_hash, state_record) in &record.state_hashes {
                        let marker = if state_hash == &record.active_state {
                            "* (active)"
                        } else {
                            ""
                        };
                        println!(
                            "  - {:<16} (installed at: {}) {}",
                            state_hash, state_record.installed_at, marker
                        );
                    }
                }
                None => {
                    eprintln!("Package '{}' not found.", id);
                    std::process::exit(1);
                }
            }
        }
        "states" | "versions" => {
            if args.len() < 3 {
                eprintln!("Error: Missing package ID.");
                eprintln!("Usage: bpm states <package_id>");
                std::process::exit(1);
            }
            let id = &args[2];
            let states = manager.get_installed_states(id);
            if states.is_empty() {
                println!("Package '{}' has no installed states.", id);
            } else {
                for state in states {
                    println!("{state}");
                }
            }
        }
        "activate" => {
            if args.len() < 4 {
                eprintln!("Error: Missing arguments.");
                eprintln!("Usage: bpm activate <package_id> <state_hash>");
                std::process::exit(1);
            }
            let id = &args[2];
            let state_hash = &args[3];
            match manager.set_active(id, state_hash) {
                Ok(()) => println!("Active state for '{}' set to {}.", id, state_hash),
                Err(e) => {
                    eprintln!("Failed to activate state: {e}");
                    std::process::exit(1);
                }
            }
        }
        "uninstall" => {
            if args.len() < 4 {
                eprintln!("Error: Missing arguments.");
                eprintln!("Usage: bpm uninstall <package_id> <state_hash>");
                std::process::exit(1);
            }
            let id = &args[2];
            let state_hash = &args[3];
            match manager.uninstall(id, state_hash) {
                Ok(()) => println!("Successfully uninstalled '{}' state {}.", id, state_hash),
                Err(e) => {
                    eprintln!("Failed to uninstall package: {e}");
                    std::process::exit(1);
                }
            }
        }
        "upgrade" => {
            let sources_path = storage_dir.join("sources.json");
            let sources_cfg = SourcesConfig::load_or_init(&sources_path)
                .map_err(PackageManagerError::StorageError)?;
            let cache_mgr = RegistryCacheManager::new(&storage_dir);
            let active_sources = sources_cfg.active_sources_by_priority();
            let cached_indexes = cache_mgr.load_all_cached(&active_sources);
            let pairs: Vec<(&RegistrySource, &RemoteRegistryIndex)> =
                cached_indexes.iter().map(|(s, idx)| (s, idx)).collect();
            let remote_packages = SourcesConfig::merge_packages(&pairs);

            let updates = find_available_updates(&manager, &remote_packages);
            if updates.is_empty() {
                println!("All installed packages are up to date.");
                return Ok(());
            }

            println!("Found {} package(s) with available updates:", updates.len());
            for u in &updates {
                let short_cur = if u.current_state_hash.len() > 10 {
                    &u.current_state_hash[..10]
                } else {
                    &u.current_state_hash
                };
                let short_target = if u.target_state_hash.len() > 10 {
                    &u.target_state_hash[..10]
                } else {
                    &u.target_state_hash
                };
                println!(
                    "  - {} ({}): {} -> {} (v{})",
                    u.current_name, u.id, short_cur, short_target, u.target_version
                );
            }
            println!();

            let installer = RemotePackageInstaller::new(&storage_dir);
            let client = HttpClient::default();
            let mut success_count = 0;

            for (i, update) in updates.iter().enumerate() {
                println!(
                    "[{}/{}] Upgrading '{}' ({})...",
                    i + 1,
                    updates.len(),
                    update.current_name,
                    update.id
                );
                match installer.install_remote_package(
                    &mut manager,
                    &client,
                    &update.remote_pkg,
                    "",
                    |cur, tot| {
                        print_progress_bar("  ", cur, tot);
                    },
                ) {
                    Ok(installed) => {
                        println!();
                        let short_hash = if installed.state_hash.len() > 10 {
                            &installed.state_hash[..10]
                        } else {
                            &installed.state_hash
                        };
                        println!("  OK -> state {short_hash}");
                        success_count += 1;
                    }
                    Err(e) => {
                        println!();
                        eprintln!("  FAILED ({e})");
                    }
                }
            }

            println!(
                "Upgrade complete: {}/{} packages updated successfully.",
                success_count,
                updates.len()
            );
        }
        "source" | "sources" => {
            let sources_path = storage_dir.join("sources.json");
            let mut sources_cfg = SourcesConfig::load_or_init(&sources_path)
                .map_err(PackageManagerError::StorageError)?;

            let subcmd = args.get(2).map(|s| s.as_str()).unwrap_or("list");
            match subcmd {
                "list" => {
                    println!("{:<16} {:<10} {:<10} URL", "ID", "PRIORITY", "STATUS");
                    println!("{:-<80}", "");
                    for s in &sources_cfg.sources {
                        let status = if s.enabled { "ENABLED" } else { "DISABLED" };
                        println!("{:<16} {:<10} {:<10} {}", s.id, s.priority, status, s.url);
                    }
                }
                "add" => {
                    if args.len() < 5 {
                        eprintln!("Error: Missing source ID or URL.");
                        eprintln!(
                            "Usage: bpm source add <id> <url> [--name <name>] [--priority <priority>]"
                        );
                        std::process::exit(1);
                    }
                    let id = &args[3];
                    let url = &args[4];

                    let mut name = id.clone();
                    let mut priority = 100u32;
                    let mut i = 5;
                    while i < args.len() {
                        match args[i].as_str() {
                            "--name" => {
                                if i + 1 < args.len() {
                                    name = args[i + 1].clone();
                                    i += 2;
                                } else {
                                    i += 1;
                                }
                            }
                            "--priority" => {
                                if i + 1 < args.len() {
                                    priority = args[i + 1].parse().unwrap_or(100);
                                    i += 2;
                                } else {
                                    i += 1;
                                }
                            }
                            _ => i += 1,
                        }
                    }

                    sources_cfg.add_or_update(id, name, url, priority);
                    sources_cfg
                        .save_to_file(&sources_path)
                        .map_err(PackageManagerError::StorageError)?;
                    println!(
                        "Successfully added/updated registry source '{}' ({}).",
                        id, url
                    );

                    // Fetch index for newly added source
                    let cache_mgr = RegistryCacheManager::new(&storage_dir);
                    let client = HttpClient::default();
                    if let Some(source) = sources_cfg.find(id) {
                        print!("Fetching index for '{}'... ", id);
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                        match cache_mgr.update_or_fallback(&client, source) {
                            Ok((idx, _)) => {
                                println!("OK ({} packages indexed)", idx.packages.len());
                            }
                            Err(e) => {
                                println!(
                                    "Notice: Could not fetch immediately ({e}). Run 'bpm update' later."
                                );
                            }
                        }
                    }
                }
                "remove" => {
                    if args.len() < 4 {
                        eprintln!("Error: Missing source ID.");
                        eprintln!("Usage: bpm source remove <id>");
                        std::process::exit(1);
                    }
                    let id = &args[3];
                    if sources_cfg.remove(id) {
                        sources_cfg
                            .save_to_file(&sources_path)
                            .map_err(PackageManagerError::StorageError)?;
                        // Clean up cached index file if present
                        let cache_mgr = RegistryCacheManager::new(&storage_dir);
                        let cache_file = cache_mgr.cache_path_for_source(id);
                        let _ = fs::remove_file(cache_file);
                        println!("Successfully removed registry source '{}'.", id);
                    } else {
                        eprintln!("Error: Registry source '{}' not found.", id);
                        std::process::exit(1);
                    }
                }
                other => {
                    eprintln!("Unknown source command: '{other}'");
                    eprintln!("Usage: bpm source <list|add|remove>");
                    std::process::exit(1);
                }
            }
        }
        "export" => {
            if args.len() < 3 {
                eprintln!("Error: Missing package path or ID.");
                eprintln!("Usage: bpm export <package_path_or_id> [-o <output_dir>]");
                std::process::exit(1);
            }
            let target = &args[2];
            let out_dir = if let Some(o_idx) = args
                .iter()
                .position(|a| a == "-o" || a == "--output" || a == "--to")
            {
                if o_idx + 1 < args.len() {
                    args[o_idx + 1].clone()
                } else {
                    format!("{}_exported", target.trim_end_matches(".bmsp"))
                }
            } else {
                format!("{}_exported", target.trim_end_matches(".bmsp"))
            };

            println!("Exporting '{}' to '{}'...", target, out_dir);
            match manager.export_package(target, &out_dir) {
                Ok(stats) => {
                    println!(
                        "Export completed successfully! Total: {} files ({} charts, {} WAVs, {} BGAs, {} videos, {} others)",
                        stats.total_files, stats.bms_files, stats.wav_files, stats.bga_files, stats.video_files, stats.other_files
                    );
                }
                Err(e) => {
                    eprintln!("Export failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        "mount" => {
            let mut port = 8989u16;
            let mut drive_letter: Option<String> = None;
            let mut target_path: Option<String> = None;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--port" | "-p" => {
                        if i + 1 < args.len() {
                            port = args[i + 1].parse().unwrap_or(8989);
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    "--drive" | "-d" => {
                        if i + 1 < args.len() {
                            drive_letter = Some(args[i + 1].trim_end_matches(':').to_string());
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    other if !other.starts_with('-') => {
                        target_path = Some(other.to_string());
                        i += 1;
                    }
                    _ => {
                        i += 1;
                    }
                }
            }

            let mut vfs = bms_package_manager::VirtualBmsFs::new();
            if let Some(target) = target_path {
                let p = std::path::Path::new(&target);
                if p.is_file() {
                    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("song");
                    vfs.mount_package(stem, p)?;
                    println!("Mounted single package '{}' as '/{}'", p.display(), stem);
                } else if p.is_dir() {
                    let count = vfs.mount_directory(p)?;
                    println!(
                        "Mounted {} packages from directory '{}'",
                        count,
                        p.display()
                    );
                }
            } else {
                vfs = manager.create_vfs()?;
                println!(
                    "Mounted all active library packages from '{}'",
                    storage_dir.display()
                );
            }

            let vfs_arc = std::sync::Arc::new(vfs);
            let server = bms_package_manager::WebDavServer::start(vfs_arc, port).map_err(|e| {
                PackageManagerError::StorageError(format!("Failed to start WebDAV server: {e}"))
            })?;

            let actual_port = server.port();
            println!(
                "WebDAV VFS server listening on http://127.0.0.1:{}/",
                actual_port
            );

            if let Some(drive) = drive_letter {
                #[cfg(target_os = "windows")]
                {
                    println!(
                        "Mounting virtual network drive {}: -> http://127.0.0.1:{}/...",
                        drive, actual_port
                    );
                    let cmd = format!(
                        "net use {}: http://127.0.0.1:{}/ /persistent:no",
                        drive, actual_port
                    );
                    let _ = std::process::Command::new("cmd")
                        .args(["/C", &cmd])
                        .status();
                }
                #[cfg(not(target_os = "windows"))]
                {
                    println!(
                        "Network drive mounting via drive letter is only supported on Windows."
                    );
                }
            }

            println!("VFS active! Legacy players (LR2, beatoraja) can read virtual WAV/BMP files.");
            println!("Press Ctrl+C to terminate VFS daemon.");

            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        "unmount" => {
            let drive = args
                .iter()
                .position(|a| a == "--drive" || a == "-d")
                .and_then(|idx| args.get(idx + 1))
                .map(|d| d.trim_end_matches(':'))
                .unwrap_or("Z");

            #[cfg(target_os = "windows")]
            {
                println!("Unmounting virtual drive {}:...", drive);
                let cmd = format!("net use {}: /delete /y", drive);
                let status = std::process::Command::new("cmd")
                    .args(["/C", &cmd])
                    .status();
                match status {
                    Ok(s) if s.success() => println!("Successfully unmounted {}:", drive),
                    _ => println!("Drive {}: unmounted (or was not mounted).", drive),
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                println!("Unmount command is only needed on Windows.");
            }
        }
        "serve" => {
            let mut port = 8080u16;
            let mut bind_addr = "0.0.0.0".to_string();

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--port" | "-p" => {
                        if i + 1 < args.len() {
                            port = args[i + 1].parse().unwrap_or(8080);
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    "--bind" | "-b" => {
                        if i + 1 < args.len() {
                            bind_addr = args[i + 1].clone();
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    _ => i += 1,
                }
            }

            println!("Starting Beetle Local LAN Package Hub...");
            let server = match bms_package_manager::BmsServeServer::start(
                storage_dir.clone(),
                &bind_addr,
                port,
            ) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to start serve daemon: {e}");
                    std::process::exit(1);
                }
            };

            let actual_port = server.port();
            let host_ip = if bind_addr == "0.0.0.0" {
                bms_package_manager::get_local_ip()
                    .map(|ip| ip.to_string())
                    .unwrap_or_else(|| "127.0.0.1".to_string())
            } else {
                bind_addr.clone()
            };

            println!("======================================================================");
            println!("  BMS Package Manager (bpm serve) - Local LAN Registry Hub");
            println!("======================================================================");
            println!("  Local Web URL:  http://{host_ip}:{actual_port}/");
            println!("  Registry URL:   http://{host_ip}:{actual_port}/index.json");
            println!("  Storage Root:   {}", storage_dir.display());
            println!();
            println!("  Other devices on the same Wi-Fi / LAN can add this source:");
            println!("    bpm source add lan http://{host_ip}:{actual_port}/index.json");
            println!("======================================================================");
            println!("Press Ctrl+C to terminate the LAN server.");

            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        other => {
            eprintln!("Unknown command: '{other}'");
            print_usage();
            std::process::exit(1);
        }
    }

    Ok(())
}
