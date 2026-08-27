use bms_package_manager::{PackageManager, PackageManagerError};
use std::env;
use std::path::PathBuf;

fn print_usage() {
    println!("BMS Package Manager (bpm)");
    println!();
    println!("Usage:");
    println!("  bpm install <package.bmsp>    Install a local .bmsp package");
    println!("  bpm import <folder_path>      Import an existing BMS folder into managed storage");
    println!("  bpm pack <folder> [-o <out>]  Pack a BMS folder into a .bmsp archive");
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
        "pack" => {
            if args.len() < 3 {
                eprintln!("Error: Missing folder path.");
                eprintln!("Usage: bpm pack <folder_path> [-o <output.bmsp>]");
                std::process::exit(1);
            }
            let folder = &args[2];
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
                    if let Err(e) = std::fs::write(&out_file, bytes) {
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
                        "Successfully imported and installed '{}' v{} ({})",
                        installed.name, installed.version, installed.id
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
