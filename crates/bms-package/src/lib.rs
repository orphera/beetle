//! # bms-package
//!
//! Standardized BMS package format (`.bmsp`) container, manifest, and archive library.
//! Designed for secure, deterministic, self-contained packaging and distribution of BMS content.

pub mod builder;
pub mod entry;
pub mod error;
pub mod manifest;
pub mod path;
pub mod reader;

pub use builder::PackageBuilder;
pub use entry::PackageEntry;
pub use error::PackageError;
pub use manifest::{Manifest, CURRENT_FORMAT_VERSION, MANIFEST_FILENAME};
pub use path::validate_entry_path;
pub use reader::{Package, DEFAULT_MAX_ENTRY_SIZE};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_package_roundtrip_and_determinism() {
        let manifest = Manifest::new("com.example.testsong", "1.0.0", "Test Song")
            .with_author("Beetle")
            .with_extra("genre", serde_json::json!("Electronic"));

        // Build package 1
        let mut builder1 = PackageBuilder::new(manifest.clone());
        builder1
            .add_file("bms/main.bms", b"#TITLE Test Song\n#BPM 150".to_vec())
            .unwrap();
        builder1
            .add_file("audio/01.wav", vec![0u8; 100])
            .unwrap();
        builder1
            .add_file("image/stage.png", vec![255u8; 50])
            .unwrap();

        let bytes1 = builder1.build_to_bytes().unwrap();

        // Build package 2 (added in completely reversed order)
        let mut builder2 = PackageBuilder::new(manifest);
        builder2
            .add_file("image/stage.png", vec![255u8; 50])
            .unwrap();
        builder2
            .add_file("audio/01.wav", vec![0u8; 100])
            .unwrap();
        builder2
            .add_file("bms/main.bms", b"#TITLE Test Song\n#BPM 150".to_vec())
            .unwrap();

        let bytes2 = builder2.build_to_bytes().unwrap();

        // Test determinism (byte-for-byte exact equality)
        assert_eq!(bytes1, bytes2, "Package builds must be 100% deterministic");

        // Read and verify package
        let pkg = Package::from_bytes(bytes1).unwrap();
        assert_eq!(pkg.manifest().id, "com.example.testsong");
        assert_eq!(pkg.manifest().version, "1.0.0");
        assert_eq!(pkg.manifest().name, "Test Song");
        assert_eq!(pkg.manifest().author, Some("Beetle".to_string()));

        assert!(pkg.contains("bms/main.bms"));
        assert!(pkg.contains("audio/01.wav"));
        assert!(pkg.contains("image/stage.png"));
        assert!(!pkg.contains("non_existent.file"));

        let bms_data = pkg.read_entry("bms/main.bms").unwrap();
        assert_eq!(bms_data, b"#TITLE Test Song\n#BPM 150");

        // Test streaming open_entry
        let mut stream = pkg.open_entry("bms/main.bms").unwrap();
        let mut stream_content = Vec::new();
        std::io::Read::read_to_end(&mut stream, &mut stream_content).unwrap();
        assert_eq!(stream_content, b"#TITLE Test Song\n#BPM 150");
    }

    #[test]
    fn test_missing_manifest_rejected() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            zip.start_file("bms/main.bms", SimpleFileOptions::default()).unwrap();
            zip.write_all(b"test").unwrap();
            zip.finish().unwrap();
        }

        let zip_bytes = cursor.into_inner();
        let result = Package::from_bytes(zip_bytes);
        assert!(matches!(result, Err(PackageError::MissingManifest)));
    }

    #[test]
    fn test_invalid_json_manifest_rejected() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            zip.start_file(MANIFEST_FILENAME, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"{ not a valid json").unwrap();
            zip.finish().unwrap();
        }

        let zip_bytes = cursor.into_inner();
        let result = Package::from_bytes(zip_bytes);
        assert!(matches!(result, Err(PackageError::InvalidManifest(_))));
    }

    #[test]
    fn test_unsupported_format_version_rejected() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            zip.start_file(MANIFEST_FILENAME, SimpleFileOptions::default()).unwrap();
            zip.write_all(br#"{"format": 999, "id": "test", "version": "1.0.0", "name": "Test"}"#).unwrap();
            zip.finish().unwrap();
        }

        let zip_bytes = cursor.into_inner();
        let result = Package::from_bytes(zip_bytes);
        assert!(matches!(result, Err(PackageError::UnsupportedFormat(999))));
    }

    #[test]
    fn test_path_traversal_in_zip_rejected() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            zip.start_file(MANIFEST_FILENAME, SimpleFileOptions::default()).unwrap();
            zip.write_all(br#"{"format": 1, "id": "test", "version": "1.0.0", "name": "Test"}"#).unwrap();
            zip.start_file("../outside.txt", SimpleFileOptions::default()).unwrap();
            zip.write_all(b"malicious").unwrap();
            zip.finish().unwrap();
        }

        let zip_bytes = cursor.into_inner();
        let result = Package::from_bytes(zip_bytes);
        assert!(matches!(result, Err(PackageError::InvalidEntryPath(_))));
    }

    #[test]
    fn test_duplicate_entry_in_builder_rejected() {
        let manifest = Manifest::new("test", "1.0.0", "Test");
        let mut builder = PackageBuilder::new(manifest);
        builder.add_file("audio/01.wav", vec![0u8; 10]).unwrap();
        let result = builder.add_file("audio/01.wav", vec![1u8; 10]);
        assert!(matches!(result, Err(PackageError::DuplicateEntry(_))));
    }

    #[test]
    fn test_builder_rejects_manual_manifest_entry() {
        let manifest = Manifest::new("test", "1.0.0", "Test");
        let mut builder = PackageBuilder::new(manifest);
        let result = builder.add_file(MANIFEST_FILENAME, vec![0u8; 10]);
        assert!(matches!(result, Err(PackageError::InvalidEntryPath(_))));
    }
}
