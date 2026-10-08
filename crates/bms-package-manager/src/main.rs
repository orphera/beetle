use bms_package_manager::{
    fetch_table, find_available_updates, HttpClient, PackageManager, PackageManagerError, PackageUpdater,
    RegistryCacheManager, RegistrySource, RemotePackageInstaller, RemoteRegistryIndex,
    absolute_dir, load_library, save_library, SourcesConfig, TableStore, UpdateOutcome,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn print_usage() {
    println!("BMS Package Manager (bpm)");
    println!();
    println!("Usage:");
    println!("  bpm install <package.bmsp_or_id> [--with-bga] Install local package or download from remote registry");
    println!("  bpm update [delta.bmdp]                Update remote registry indexes (or apply a delta package)");
    println!("  bpm search <query>                     Search remote packages across configured registries");
    println!("  bpm upgrade                            Batch upgrade installed packages to latest remote versions");
    println!("  bpm source <list|add|remove>           Manage remote registry sources (sources.json)");
    println!("  bpm table <add|update|list|remove>     Manage difficulty tables (tables/, read by the player)");
    println!("  bpm library <add|list|remove> [path]   Register existing BMS folders the player scans in place (no copy)");
    println!("  bpm import <folder_path>               Import an existing BMS folder into managed storage");
    println!("  bpm pack <folder> [-o <out>] [--turbo] [--flac] [--split-bga] [--no-video] Pack a BMS folder into a .bmsp archive");
    println!("  bpm diff <base> <target> [-o <out>]    Generate a .bmdp delta package between states/folders");
    println!(
        "  bpm patch <base> <diff> [-o <out>]     Reconstruct a target .bmsp from base + diff"
    );
    println!("  bpm export <package_or_id> [-o <dir>]  Export package back into traditional BMS folder structure");
    println!("  bpm bga install <package.bga.bmsp_or_id> Install a decoupled BGA companion package");
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
    use std::path::Path;
    env::var("BEETLE_PACKAGES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            for candidate in &["packages", "target/release/packages", "../packages"] {
                let p = Path::new(candidate);
                if p.join("registry.json").exists() {
                    return p.to_path_buf();
                }
            }
            PathBuf::from("packages")
        })
}

fn print_table_usage() {
    println!("Usage:");
    println!("  bpm table add <address>             Install a difficulty table from its page or header.json");
    println!("  bpm table update [name] [--force]   Fetch installed tables again (all, or one)");
    println!("  bpm table list                      List installed tables");
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
                let status = if Path::new(p).is_dir() { "ok" } else { "missing" };
                println!("[{status}] {p}");
            }
        }
        _ => {
            eprintln!("Usage: bpm library <add|remove> <folder> | bpm library list");
            eprintln!("Folders are kept in ./library.txt (or $BEETLE_LIBRARY_FILE), scanned in place by the player.");
            std::process::exit(1);
        }
    }
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
                Some(name) => vec![store.find(name).unwrap_or_else(|| fail(&format!("no installed table named '{name}'")))],
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
                    Ok(UpdateOutcome::Updated { entries, previous_entries }) => {
                        println!("{name}: {entries} charts ({:+} since the last update)", entries as i64 - previous_entries as i64);
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
        _ => print_table_usage(),
    }
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
            let pairs: Vec<(&RegistrySource, &RemoteRegistryIndex)> = cached_indexes
                .iter()
                .map(|(s, idx)| (s, idx))
                .collect();
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
                    println!("No packages found in registry cache. Run 'bpm update' to fetch indexes.");
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
                eprintln!("Error: Missing folder path.");
                eprintln!("Usage: bpm import <folder_path>");
                std::process::exit(1);
            }
            let folder = &args[2];
            let roots = bms_package_manager::find_bms_song_roots(folder);
            if roots.is_empty() {
                eprintln!("Error: No BMS chart files found in '{}'", folder);
                std::process::exit(1);
            }

            if roots.len() == 1 {
                let target_root = &roots[0];
                if target_root != Path::new(folder) {
                    println!("Detected BMS song root at '{}'", target_root.display());
                }
                match manager.import_folder(target_root, None) {
                    Ok(installed) => {
                        println!(
                            "Successfully imported and installed '{}' ({}) -> state {}",
                            installed.name, installed.id, installed.state_hash
                        );
                        println!("Location: {}", installed.location.display());
                    }
                    Err(e) => {
                        eprintln!("Import failed: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                println!(
                    "Found {} BMS song directories under '{}'. Batch importing each...",
                    roots.len(),
                    folder
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

                let pairs: Vec<(&RegistrySource, &RemoteRegistryIndex)> = cached_indexes
                    .iter()
                    .map(|(s, idx)| (s, idx))
                    .collect();
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
                        let pairs: Vec<(&RegistrySource, &RemoteRegistryIndex)> = cached_indexes
                            .iter()
                            .map(|(s, idx)| (s, idx))
                            .collect();
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
            let pairs: Vec<(&RegistrySource, &RemoteRegistryIndex)> = cached_indexes
                .iter()
                .map(|(s, idx)| (s, idx))
                .collect();
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
