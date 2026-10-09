use crate::manager::{InstalledPackage, PackageManager};
use crate::net::http::HttpClient;
use crate::net::installer::RemotePackageInstaller;
use crate::registry::remote::RemotePackageMetadata;

/// Information about a detected available package update.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageUpdateInfo {
    pub id: String,
    pub current_name: String,
    pub current_state_hash: String,
    pub target_version: String,
    pub target_state_hash: String,
    pub remote_pkg: RemotePackageMetadata,
}

/// Compares locally installed active packages against remote package metadata to detect available updates.
pub fn find_available_updates(
    manager: &PackageManager,
    remote_packages: &[RemotePackageMetadata],
) -> Vec<PackageUpdateInfo> {
    let mut updates = Vec::new();
    let installed_packages = manager.list_active_packages();

    for installed in installed_packages {
        if let Some(remote) = remote_packages
            .iter()
            .find(|r| r.id.eq_ignore_ascii_case(&installed.id))
        {
            // Check if the remote state hash is already installed in any state
            let already_has_state = manager
                .registry()
                .get_package(&installed.id)
                .map(|p| p.state_hashes.contains_key(&remote.state_hash))
                .unwrap_or(false);

            if !already_has_state && remote.state_hash != installed.state_hash {
                updates.push(PackageUpdateInfo {
                    id: installed.id.clone(),
                    current_name: installed.name.clone(),
                    current_state_hash: installed.state_hash.clone(),
                    target_version: remote.version.clone(),
                    target_state_hash: remote.state_hash.clone(),
                    remote_pkg: remote.clone(),
                });
            }
        }
    }

    updates.sort_by(|a, b| a.id.cmp(&b.id));
    updates
}

/// Batch upgrades all identified packages.
pub fn upgrade_packages<C: FnMut(&str, u64, Option<u64>)>(
    installer: &RemotePackageInstaller,
    manager: &mut PackageManager,
    client: &HttpClient,
    updates: &[PackageUpdateInfo],
    base_registry_url: &str,
    mut progress_reporter: C,
) -> Vec<Result<InstalledPackage, String>> {
    let mut results = Vec::new();

    for update in updates {
        let pkg_id = update.id.clone();
        let callback = |cur: u64, total: Option<u64>| {
            progress_reporter(&pkg_id, cur, total);
        };

        let res = installer.install_remote_package(
            manager,
            client,
            &update.remote_pkg,
            base_registry_url,
            callback,
        );
        results.push(res);
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_available_updates_detects_hash_difference() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_updater_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let mut manager = PackageManager::new(&temp_dir).unwrap();

        // Simulate installed package
        let pkg_bytes =
            bms_package::PackageBuilder::new(bms_package::Manifest::new("test.song", "Test Song"))
                .add_file("song.bms", b"#TITLE Test".to_vec())
                .unwrap()
                .build_to_bytes()
                .unwrap();

        let installed = manager.install_from_bytes(pkg_bytes).unwrap();
        let current_hash = installed.state_hash.clone();

        // 1. Remote has identical hash -> No update
        let same_remote = vec![RemotePackageMetadata {
            id: "test.song".to_string(),
            version: "1.0.0".to_string(),
            state_hash: current_hash.clone(),
            title: "Test Song".to_string(),
            artist: "Author".to_string(),
            genre: None,
            bpm: None,
            play_levels: vec![],
            keysounds_count: None,
            size_bytes: 100,
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            download_url: "url".to_string(),
            preview_audio_url: None,
            banner_image_url: None,
            companion_bga: None,
        }];
        assert!(find_available_updates(&manager, &same_remote).is_empty());

        // 2. Remote has new hash -> Update available!
        let new_remote = vec![RemotePackageMetadata {
            id: "test.song".to_string(),
            version: "1.1.0".to_string(),
            state_hash: "brand_new_hash_9999".to_string(),
            title: "Test Song (Remaster)".to_string(),
            artist: "Author".to_string(),
            genre: None,
            bpm: None,
            play_levels: vec![],
            keysounds_count: None,
            size_bytes: 120,
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            download_url: "url2".to_string(),
            preview_audio_url: None,
            banner_image_url: None,
            companion_bga: None,
        }];

        let updates = find_available_updates(&manager, &new_remote);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].id, "test.song");
        assert_eq!(updates[0].current_state_hash, current_hash);
        assert_eq!(updates[0].target_state_hash, "brand_new_hash_9999");
        assert_eq!(updates[0].target_version, "1.1.0");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
