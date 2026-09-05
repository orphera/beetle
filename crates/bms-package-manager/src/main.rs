use bms_package_manager::{PackageManager, PackageManagerError, PackageUpdater};
use std::env;
use std::fs;
use std::path::PathBuf;

fn print_usage() {
    println!("BMS Package Manager (bpm)");
    println!();
    println!("Usage:");
    println!("  bpm install <package.bmsp>             Install a local .bmsp package");
    println!("  bpm import <folder_path>               Import an existing BMS folder into managed storage");
    println!("  bpm pack <folder> [-o <out>] [--turbo] Pack a BMS folder into a .bmsp archive (Classic or Turbo Dual Atlas)");
    println!("  bpm diff <base> <target> [-o <out>]    Generate a .bmdp delta package between states/folders");
    println!("  bpm patch <base> <diff> [-o <out>]     Reconstruct a target .bmsp from base + diff");
    println!("  bpm export <package_or_id> [-o <dir>]  Export package back into traditional BMS folder structure");
    println!("  bpm mount [--port <port>] [--drive <Z:>] Mount packages onto on-the-fly virtual VFS drive");
    println!("  bpm unmount [--drive <Z:>]             Unmount virtual VFS network drive");
    println!("  bpm list                               List all active installed packages");
    println!("  bpm info <package_id>                  Show package metadata and installed states");
    println!("  bpm states <package_id>                List all installed states of a package");
    println!("  bpm activate <id> <state_hash>         Switch active state for a package");
    println!("  bpm uninstall <id> <state_hash>        Uninstall a specific package state");
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

fn main() -> Result<(), PackageManagerError> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let storage_dir = get_default_packages_dir();
    let mut manager = PackageManager::new(&storage_dir)?;

    match args[1].as_str() {
        "pack" => {
            if args.len() < 3 {
                eprintln!("Error: Missing folder path.");
                eprintln!("Usage: bpm pack <folder_path> [-o <output.bmsp>] [--base <base.bmsp_or_dir>]");
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
                        println!("Successfully generated delta '{}' based on '{}'", out_file, base_path);
                    }
                    Err(e) => {
                        eprintln!("Delta creation failed: {e}");
                        std::process::exit(1);
                    }
                }
                return Ok(());
            }

            let is_turbo = args.iter().any(|a| a == "--atlas" || a == "--turbo")
                || args.windows(2).any(|w| w[0] == "--profile" && w[1] == "turbo");
            let profile = if is_turbo {
                bms_package_manager::PackProfile::Turbo
            } else {
                bms_package_manager::PackProfile::Classic
            };

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

            if roots.len() == 1 {
                let target_root = &roots[0];
                let folder_name = target_root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("package")
                    .to_string();

                let out_file = if let Some(idx) = out_idx {
                    if idx + 1 < args.len() {
                        args[idx + 1].clone()
                    } else {
                        format!("{}.bmsp", folder_name)
                    }
                } else {
                    format!("{}.bmsp", folder_name)
                };

                if target_root != std::path::Path::new(folder) {
                    println!("Detected BMS song root at '{}'", target_root.display());
                }

                match manager.pack_folder_profile(target_root, None, profile) {
                    Ok(bytes) => {
                        if let Err(e) = fs::write(&out_file, bytes) {
                            eprintln!("Failed to write output package file: {e}");
                            std::process::exit(1);
                        }
                        println!("Successfully packed '{}' into '{}' [{}]", target_root.display(), out_file, profile_tag);
                    }
                    Err(e) => {
                        eprintln!("Packaging failed: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                println!("Found {} BMS song directories under '{}'. Batch packing each song...", roots.len(), folder);
                let mut success_count = 0;
                for (i, target_root) in roots.iter().enumerate() {
                    let folder_name = target_root
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("song")
                        .to_string();
                    let out_file = format!("{}.bmsp", folder_name);

                    print!("[{}/{}] Packing '{}' into '{}' [{}]... ", i + 1, roots.len(), target_root.display(), out_file, profile_tag);
                    match manager.pack_folder_profile(target_root, None, profile) {
                        Ok(bytes) => {
                            if let Err(e) = fs::write(&out_file, &bytes) {
                                println!("FAILED (Write error: {})", e);
                            } else {
                                println!("OK ({} bytes)", bytes.len());
                                success_count += 1;
                            }
                        }
                        Err(e) => {
                            println!("FAILED ({})", e);
                        }
                    }
                }
                println!("Batch packing finished: {}/{} packages created successfully.", success_count, roots.len());
            }
        }
        "diff" => {
            if args.len() < 4 {
                eprintln!("Error: Missing arguments.");
                eprintln!("Usage: bpm diff <base_path_or_bmsp> <target_path_or_bmsp> [-o <diff.bmdp>]");
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
            if args.len() < 3 {
                eprintln!("Error: Missing delta package path.");
                eprintln!("Usage: bpm update <delta.bmdp>");
                std::process::exit(1);
            }
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
        }
        "import" => {
            if args.len() < 3 {
                eprintln!("Error: Missing folder path.");
                eprintln!("Usage: bpm import <folder_path>");
                std::process::exit(1);
            }
            let folder = &args[2];
            match manager.import_folder(folder, None) {
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
        }
        "install" => {
            if args.len() < 3 {
                eprintln!("Error: Missing package file path.");
                eprintln!("Usage: bpm install <package.bmsp>");
                std::process::exit(1);
            }
            let path = &args[2];
            match manager.install(path) {
                Ok(installed) => {
                    println!(
                        "Successfully installed '{}' ({}) -> state {}",
                        installed.name, installed.id, installed.state_hash
                    );
                    println!("Location: {}", installed.location.display());
                }
                Err(e) => {
                    eprintln!("Installation failed: {e}");
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

            println!("{:<25} {:<16} {:<30} {}", "ID", "STATE", "NAME", "AUTHOR");
            println!("{:-<80}", "");
            for pkg in packages {
                let author = pkg.author.as_deref().unwrap_or("-");
                let short_hash = if pkg.state_hash.len() > 12 {
                    &pkg.state_hash[..12]
                } else {
                    &pkg.state_hash
                };
                println!(
                    "{:<25} {:<16} {:<30} {}",
                    pkg.id, short_hash, pkg.name, author
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
                    println!("Author:          {}", record.author.as_deref().unwrap_or("-"));
                    println!("Active State:    {}", record.active_state);
                    println!("Installed States:");
                    for (state_hash, state_record) in &record.state_hashes {
                        let marker = if state_hash == &record.active_state { "* (active)" } else { "" };
                        println!("  - {:<16} (installed at: {}) {}", state_hash, state_record.installed_at, marker);
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
        "export" => {
            if args.len() < 3 {
                eprintln!("Error: Missing package path or ID.");
                eprintln!("Usage: bpm export <package_path_or_id> [-o <output_dir>]");
                std::process::exit(1);
            }
            let target = &args[2];
            let out_dir = if let Some(o_idx) = args.iter().position(|a| a == "-o" || a == "--output" || a == "--to") {
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
                    println!("Mounted {} packages from directory '{}'", count, p.display());
                }
            } else {
                vfs = manager.create_vfs()?;
                println!("Mounted all active library packages from '{}'", storage_dir.display());
            }

            let vfs_arc = std::sync::Arc::new(vfs);
            let server = bms_package_manager::WebDavServer::start(vfs_arc, port)
                .map_err(|e| PackageManagerError::StorageError(format!("Failed to start WebDAV server: {e}")))?;

            let actual_port = server.port();
            println!("WebDAV VFS server listening on http://127.0.0.1:{}/", actual_port);

            if let Some(drive) = drive_letter {
                #[cfg(target_os = "windows")]
                {
                    println!("Mounting virtual network drive {}: -> http://127.0.0.1:{}/...", drive, actual_port);
                    let cmd = format!("net use {}: http://127.0.0.1:{}/ /persistent:no", drive, actual_port);
                    let _ = std::process::Command::new("cmd").args(["/C", &cmd]).status();
                }
                #[cfg(not(target_os = "windows"))]
                {
                    println!("Network drive mounting via drive letter is only supported on Windows.");
                }
            }

            println!("VFS active! Legacy players (LR2, beatoraja) can read virtual WAV/BMP files.");
            println!("Press Ctrl+C to terminate VFS daemon.");

            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        "unmount" => {
            let drive = args.iter().position(|a| a == "--drive" || a == "-d")
                .and_then(|idx| args.get(idx + 1))
                .map(|d| d.trim_end_matches(':'))
                .unwrap_or("Z");

            #[cfg(target_os = "windows")]
            {
                println!("Unmounting virtual drive {}:...", drive);
                let cmd = format!("net use {}: /delete /y", drive);
                let status = std::process::Command::new("cmd").args(["/C", &cmd]).status();
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
        other => {
            eprintln!("Unknown command: '{other}'");
            print_usage();
            std::process::exit(1);
        }
    }

    Ok(())
}
