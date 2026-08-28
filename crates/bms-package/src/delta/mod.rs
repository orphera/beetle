pub mod applicator;
pub mod builder;
pub mod manifest;

pub use applicator::{DeltaApplicator, DeltaPackage};
pub use builder::DeltaBuilder;
pub use manifest::{
    DeltaManifest, DeltaOpKind, DeltaResourceEntry, CURRENT_DELTA_FORMAT_VERSION,
    DELTA_MANIFEST_FILENAME,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::PackageBuilder;
    use crate::checksum::sha256_hex;
    use crate::error::PackageError;
    use crate::manifest::Manifest;
    use crate::reader::Package;

    #[test]
    fn test_delta_diff_and_apply_roundtrip_determinism() {
        // 1. Build Base Package
        let manifest_v1 = Manifest::new("com.example.song", "Example Song")
            .with_author("Composer");
        let kick_data: Vec<u8> = (0..20000).map(|i| (i % 251) as u8).collect();

        let mut builder_v1 = PackageBuilder::new(manifest_v1);
        builder_v1
            .add_file("bms/normal.bms", b"#TITLE Example Song\n#BPM 140".to_vec())
            .unwrap();
        builder_v1
            .add_file("audio/kick.wav", kick_data.clone())
            .unwrap();
        builder_v1
            .add_file("audio/snare.wav", vec![2u8; 500])
            .unwrap();
        builder_v1
            .add_file("image/old_banner.png", vec![3u8; 200])
            .unwrap();
        let bytes_v1 = builder_v1.build_to_bytes().unwrap();
        let hash_v1 = sha256_hex(&bytes_v1);

        // 2. Build Target Package:
        // - "bms/normal.bms": unchanged
        // - "audio/kick.wav": unchanged (20 KB not included in delta!)
        // - "audio/snare.wav": modified (remastered)
        // - "image/old_banner.png": removed
        // - "bms/insane.bms": added (new chart)
        let manifest_v2 = Manifest::new("com.example.song", "Example Song (Remaster)")
            .with_author("Composer");
        let mut builder_v2 = PackageBuilder::new(manifest_v2);
        builder_v2
            .add_file("bms/normal.bms", b"#TITLE Example Song\n#BPM 140".to_vec())
            .unwrap();
        builder_v2
            .add_file("audio/kick.wav", kick_data.clone())
            .unwrap();
        builder_v2
            .add_file("audio/snare.wav", vec![9u8; 600])
            .unwrap(); // modified
        builder_v2
            .add_file("bms/insane.bms", b"#TITLE Example Song\n#BPM 140\n#PLAYLEVEL 12".to_vec())
            .unwrap(); // added
        let bytes_v2 = builder_v2.build_to_bytes().unwrap();
        let hash_v2 = sha256_hex(&bytes_v2);

        // 3. Compute Delta (.bmdp)
        let base_pkg = Package::from_bytes(bytes_v1.clone()).unwrap();
        let target_pkg = Package::from_bytes(bytes_v2.clone()).unwrap();

        let delta_builder = DeltaBuilder::from_packages(&base_pkg, &target_pkg)
            .unwrap()
            .with_checksums(Some(hash_v1.clone()), Some(hash_v2.clone()));

        assert_eq!(delta_builder.manifest().resources.len(), 5);
        let delta_bytes = delta_builder.build_to_bytes().unwrap();

        // Delta should be significantly smaller than full package
        assert!(delta_bytes.len() < bytes_v2.len());

        // 4. Apply Delta onto Base Package
        let base_for_patch = Package::from_bytes(bytes_v1.clone()).unwrap();
        let mut delta_pkg = DeltaPackage::from_bytes(delta_bytes.clone()).unwrap();

        let reconstructed_bytes = DeltaApplicator::apply_to_bytes(
            &base_for_patch,
            &mut delta_pkg,
            Some(&bytes_v1),
        )
        .unwrap();

        // 5. Verify reconstructed package is 100% byte-for-byte identical to Target Package!
        assert_eq!(
            reconstructed_bytes, bytes_v2,
            "Reconstructed package must be 100% byte-for-byte identical to the original Target Package"
        );

        let reconstructed_pkg = Package::from_bytes(reconstructed_bytes).unwrap();
        assert_eq!(reconstructed_pkg.manifest().name, "Example Song (Remaster)");
        assert!(reconstructed_pkg.contains("bms/insane.bms"));
        assert!(!reconstructed_pkg.contains("image/old_banner.png"));
        assert_eq!(
            reconstructed_pkg.read_entry("audio/snare.wav").unwrap(),
            vec![9u8; 600]
        );
        assert_eq!(
            reconstructed_pkg.read_entry("audio/kick.wav").unwrap(),
            kick_data
        );
    }

    #[test]
    fn test_delta_rejects_mismatched_base_state() {
        let mut b1 = PackageBuilder::new(Manifest::new("com.example.song", "Song"));
        b1.add_file("audio/01.wav", vec![1u8; 100]).unwrap();
        let pkg_v1 = b1.build_to_bytes().unwrap();

        let mut b2 = PackageBuilder::new(Manifest::new("com.example.song", "Song"));
        b2.add_file("audio/01.wav", vec![2u8; 100]).unwrap();
        let pkg_v2 = b2.build_to_bytes().unwrap();

        let base_pkg = Package::from_bytes(pkg_v1).unwrap();
        let target_pkg = Package::from_bytes(pkg_v2).unwrap();

        let delta_builder = DeltaBuilder::from_packages(&base_pkg, &target_pkg).unwrap();
        let delta_bytes = delta_builder.build_to_bytes().unwrap();

        // Attempt to apply onto wrong base state
        let mut b_wrong = PackageBuilder::new(Manifest::new("com.example.song", "Song"));
        b_wrong.add_file("audio/01.wav", vec![3u8; 100]).unwrap();
        let pkg_wrong = b_wrong.build_to_bytes().unwrap();
        let wrong_base = Package::from_bytes(pkg_wrong).unwrap();
        let mut delta_pkg = DeltaPackage::from_bytes(delta_bytes).unwrap();

        let result = DeltaApplicator::apply_to_bytes(&wrong_base, &mut delta_pkg, None);
        assert!(matches!(result, Err(PackageError::DeltaBaseMismatch { .. })));
    }

    #[test]
    fn test_delta_rejects_corrupted_payload_checksum() {
        let manifest_v1 = Manifest::new("com.example.song", "Song");
        let mut b1 = PackageBuilder::new(manifest_v1);
        b1.add_file("audio/01.wav", vec![1u8; 100]).unwrap();
        let pkg_v1 = b1.build_to_bytes().unwrap();

        let manifest_v2 = Manifest::new("com.example.song", "Song");
        let mut b2 = PackageBuilder::new(manifest_v2.clone());
        b2.add_file("audio/01.wav", vec![2u8; 100]).unwrap();
        let pkg_v2 = b2.build_to_bytes().unwrap();

        let base_pkg = Package::from_bytes(pkg_v1.clone()).unwrap();
        let target_pkg = Package::from_bytes(pkg_v2).unwrap();

        let mut delta_builder = DeltaBuilder::from_packages(&base_pkg, &target_pkg).unwrap();
        // Tamper with payload
        delta_builder.payload_files.insert("audio/01.wav".to_string(), vec![99u8; 100]);
        let tampered_delta_bytes = delta_builder.build_to_bytes().unwrap();

        let base_for_patch = Package::from_bytes(pkg_v1).unwrap();
        let mut delta_pkg = DeltaPackage::from_bytes(tampered_delta_bytes).unwrap();

        let result = DeltaApplicator::apply_to_bytes(&base_for_patch, &mut delta_pkg, None);
        assert!(matches!(result, Err(PackageError::DeltaChecksumMismatch { .. })));
    }
}
