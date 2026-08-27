use bms_package_manager::{PackageManager, PackageManagerError};
use std::env;
use std::path::PathBuf;

fn print_usage() {
    println!("BMS Package Manager (bpm)");
    println!();
    println!("Usage:");
    println!("  bpm install <package.bmsp>    Install a local .bmsp package");
    println!("  bpm list                      List all active installed packages");
    println!("  bpm info <package_id>         Show package metadata and installed versions");
    println!("  bpm versions <package_id>     List all installed versions of a package");
    println!("  bpm activate <id> <version>   Switch active version for a package");
    println!("  bpm uninstall <id> <version>  Uninstall a specific package version");
}

fn get_default_packages_dir() -> PathBuf {
    env::var("BEETLE_PACKAGES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("packages"))
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
                        "Successfully installed '{}' v{} ({})",
                        installed.name, installed.version, installed.id
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

            println!("{:<25} {:<10} {:<30} {}", "ID", "VERSION", "NAME", "AUTHOR");
            println!("{:-<80}", "");
            for pkg in packages {
                let author = pkg.author.as_deref().unwrap_or("-");
                println!(
                    "{:<25} {:<10} {:<30} {}",
                    pkg.id, pkg.version, pkg.name, author
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
                    println!("Active Version:  {}", record.active_version);
                    println!("Installed Versions:");
                    for (ver, ver_record) in &record.versions {
                        let marker = if ver == &record.active_version { "* (active)" } else { "" };
                        println!("  - {:<10} (installed at: {}) {}", ver, ver_record.installed_at, marker);
                    }
                }
                None => {
                    eprintln!("Package '{}' not found.", id);
                    std::process::exit(1);
                }
            }
        }
        "versions" => {
            if args.len() < 3 {
                eprintln!("Error: Missing package ID.");
                eprintln!("Usage: bpm versions <package_id>");
                std::process::exit(1);
            }
            let id = &args[2];
            let versions = manager.get_installed_versions(id);
            if versions.is_empty() {
                println!("Package '{}' has no installed versions.", id);
            } else {
                for ver in versions {
                    println!("{ver}");
                }
            }
        }
        "activate" => {
            if args.len() < 4 {
                eprintln!("Error: Missing arguments.");
                eprintln!("Usage: bpm activate <package_id> <version>");
                std::process::exit(1);
            }
            let id = &args[2];
            let version = &args[3];
            match manager.set_active(id, version) {
                Ok(()) => println!("Active version for '{}' set to v{}.", id, version),
                Err(e) => {
                    eprintln!("Failed to activate version: {e}");
                    std::process::exit(1);
                }
            }
        }
        "uninstall" => {
            if args.len() < 4 {
                eprintln!("Error: Missing arguments.");
                eprintln!("Usage: bpm uninstall <package_id> <version>");
                std::process::exit(1);
            }
            let id = &args[2];
            let version = &args[3];
            match manager.uninstall(id, version) {
                Ok(()) => println!("Successfully uninstalled '{}' v{}.", id, version),
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
