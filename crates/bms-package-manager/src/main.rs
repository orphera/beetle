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
    println!("  bpm pack <folder> [-o <out>]           Pack a BMS folder into a .bmsp archive");
    println!("  bpm diff <base> <target> [-o <out>]    Generate a .bmdp delta package between states/folders");
    println!("  bpm patch <base> <diff> [-o <out>]     Reconstruct a target .bmsp from base + diff");
    println!("  bpm update <delta.bmdp>                Atomically apply a delta package to installed library");
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

            let out_file = if args.len() >= 5 && args[3] == "-o" {
                args[4].clone()
            } else {
                let folder_name = PathBuf::from(folder)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("package")
                    .to_string();
                format!("{}.bmsp", folder_name)
            };

            match manager.pack_folder(folder, None) {
                Ok(bytes) => {
                    if let Err(e) = fs::write(&out_file, bytes) {
                        eprintln!("Failed to write output package file: {e}");
                        std::process::exit(1);
                    }
                    println!("Successfully packed '{}' into '{}'", folder, out_file);
                }
                Err(e) => {
                    eprintln!("Packaging failed: {e}");
                    std::process::exit(1);
                }
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
        other => {
            eprintln!("Unknown command: '{other}'");
            print_usage();
            std::process::exit(1);
        }
    }

    Ok(())
}
