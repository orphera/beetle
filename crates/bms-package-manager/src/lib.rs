//! # bms-package-manager
//!
//! Package lifecycle, installation, registry management, and discovery layer for `.bmsp` packages.

pub mod error;
pub mod manager;
pub mod pack;
pub mod registry;
pub mod storage;
pub mod updater;

pub use error::PackageManagerError;
pub use manager::{InstalledPackage, PackageManager};
pub use pack::{analyze_bms_folder, pack_bms_folder};
pub use registry::{PackageRecord, PackageVersionRecord, Registry};
pub use storage::PackageStorage;
pub use updater::PackageUpdater;

#[cfg(test)]
mod tests {
    use super::*;
    use bms_package::{Manifest, PackageBuilder};

    fn create_test_package_bytes(id: &str, version: &str, name: &str) -> Vec<u8> {
        let manifest = Manifest::new(id, version, name).with_author("Test Author");
        let mut builder = PackageBuilder::new(manifest);
        builder
            .add_file("bms/test.bms", b"#TITLE Sample\n#BPM 150".to_vec())
            .unwrap();
        builder
            .add_file("audio/01.wav", vec![1, 2, 3, 4, 5])
            .unwrap();
        builder.build_to_bytes().unwrap()
    }

    #[test]
    fn test_package_lifecycle_install_versions_and_uninstall() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let mut manager = PackageManager::new(&temp_dir).unwrap();

        // 1. Install version 1.0.0
        let pkg_v1 = create_test_package_bytes("com.example.song", "1.0.0", "Song V1");
        let installed1 = manager.install_from_bytes(pkg_v1.clone()).unwrap();
        assert_eq!(installed1.id, "com.example.song");
        assert_eq!(installed1.version, "1.0.0");
        assert_eq!(installed1.name, "Song V1");

        // Duplicate install rejected
        let dup_res = manager.install_from_bytes(pkg_v1);
        assert!(matches!(dup_res, Err(PackageManagerError::AlreadyInstalled { .. })));

        // 2. Install version 1.1.0 (Multi-version support)
        let pkg_v2 = create_test_package_bytes("com.example.song", "1.1.0", "Song V2");
        let installed2 = manager.install_from_bytes(pkg_v2).unwrap();
        assert_eq!(installed2.version, "1.1.0");

        // Verify active version is 1.1.0
        let active = manager.get_active_package("com.example.song").unwrap();
        assert_eq!(active.version, "1.1.0");

        // Switch active version back to 1.0.0
        manager.set_active("com.example.song", "1.0.0").unwrap();
        let active_switched = manager.get_active_package("com.example.song").unwrap();
        assert_eq!(active_switched.version, "1.0.0");

        // Test opening installed package and reading entry
        let opened_pkg = active_switched.open().unwrap();
        assert!(opened_pkg.contains("bms/test.bms"));
        assert!(opened_pkg.contains("audio/01.wav"));

        // 3. Uninstall version 1.0.0
        manager.uninstall("com.example.song", "1.0.0").unwrap();
        assert_eq!(manager.get_installed_versions("com.example.song"), vec!["1.1.0"]);

        // 4. Uninstall last version (1.1.0)
        manager.uninstall("com.example.song", "1.1.0").unwrap();
        assert!(manager.get_active_package("com.example.song").is_none());
        assert!(manager.list_active_packages().is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_corrupted_package_install_leaves_registry_clean() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_test_corrupt_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let mut manager = PackageManager::new(&temp_dir).unwrap();

        // Attempt to install garbage bytes
        let invalid_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let result = manager.install_from_bytes(invalid_bytes);
        assert!(result.is_err());

        // Registry and active packages must remain completely clean
        assert!(manager.list_active_packages().is_empty());
        assert!(manager.registry().packages.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_nonexistent_uninstall_and_activate_errors() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_test_err_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let mut manager = PackageManager::new(&temp_dir).unwrap();

        // Uninstall nonexistent package
        assert!(matches!(
            manager.uninstall("nonexistent.id", "1.0.0"),
            Err(PackageManagerError::PackageNotFound(_))
        ));

        // Activate nonexistent package
        assert!(matches!(
            manager.set_active("nonexistent.id", "1.0.0"),
            Err(PackageManagerError::PackageNotFound(_))
        ));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_import_existing_bms_folder() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_test_import_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // Create a simulated BMS song folder
        let song_folder = temp_dir.join("sakura_storm");
        std::fs::create_dir_all(&song_folder).unwrap();

        let bms_content = b"#TITLE Sakura Storm\n#ARTIST Ryu*\n#GENRE Happy Hardcore\n#00111:01000000";
        std::fs::write(song_folder.join("main.bms"), bms_content).unwrap();
        std::fs::write(song_folder.join("01.wav"), vec![0x12, 0x34]).unwrap();

        let storage_dir = temp_dir.join("bpm_storage");
        let mut manager = PackageManager::new(&storage_dir).unwrap();

        // Import the folder
        let installed = manager.import_folder(&song_folder, None).unwrap();
        assert_eq!(installed.id, "ryu.sakura_storm");
        assert_eq!(installed.version, "1.0.0");
        assert_eq!(installed.name, "Sakura Storm");
        assert_eq!(installed.author, Some("Ryu*".to_string()));

        // Check active package
        let active = manager.get_active_package("ryu.sakura_storm").unwrap();
        let pkg = active.open().unwrap();
        assert!(pkg.contains("main.bms"));
        assert!(pkg.contains("01.wav"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_atomic_delta_update_and_rollback() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_test_delta_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let mut manager = PackageManager::new(&temp_dir).unwrap();

        // 1. Install Base Package v1.0.0
        let pkg_v1 = create_test_package_bytes("com.example.delta_song", "1.0.0", "Delta Song V1");
        manager.install_from_bytes(pkg_v1.clone()).unwrap();

        let active_v1 = manager.get_active_package("com.example.delta_song").unwrap();
        assert_eq!(active_v1.version, "1.0.0");

        // 2. Create Target Package v1.1.0 and generate Delta (.bmdp)
        let manifest_v2 = Manifest::new("com.example.delta_song", "1.1.0", "Delta Song V2");
        let mut builder_v2 = PackageBuilder::new(manifest_v2);
        builder_v2
            .add_file("bms/test.bms", b"#TITLE Sample\n#BPM 150".to_vec())
            .unwrap();
        builder_v2
            .add_file("audio/01.wav", vec![9, 9, 9]) // modified
            .unwrap();
        builder_v2
            .add_file("bms/insane.bms", b"#TITLE Insane\n#PLAYLEVEL 12".to_vec()) // added
            .unwrap();
        let pkg_v2 = builder_v2.build_to_bytes().unwrap();

        let delta_bytes = PackageUpdater::create_delta_between_packages(&pkg_v1, &pkg_v2).unwrap();

        // 3. Apply Delta onto PackageManager
        let installed_v2 = manager.apply_delta_bytes(&delta_bytes).unwrap();
        assert_eq!(installed_v2.version, "1.1.0");

        let active_v2 = manager.get_active_package("com.example.delta_song").unwrap();
        assert_eq!(active_v2.version, "1.1.0");

        let opened_v2 = active_v2.open().unwrap();
        assert!(opened_v2.contains("bms/insane.bms"));
        assert_eq!(opened_v2.read_entry("audio/01.wav").unwrap(), vec![9, 9, 9]);

        // 4. Base v1.0.0 remains intact in version history
        assert_eq!(
            manager.get_installed_versions("com.example.delta_song"),
            vec!["1.0.0", "1.1.0"]
        );

        // 5. Try applying delta onto non-existent base package
        let orphan_v1 = create_test_package_bytes("com.other.orphan", "1.0.0", "Orphan");
        let orphan_v2 = create_test_package_bytes("com.other.orphan", "1.1.0", "Orphan V2");
        let orphan_delta = PackageUpdater::create_delta_between_packages(&orphan_v1, &orphan_v2).unwrap();

        let orphan_res = manager.apply_delta_bytes(&orphan_delta);
        assert!(matches!(orphan_res, Err(PackageManagerError::BaseVersionNotInstalled { .. })));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
