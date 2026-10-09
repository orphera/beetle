//! The installed package states that the registry marks as active.
//!
//! The package manager writes `registry.json` next to the installed `.bmsp`
//! files; the game reads it to know which states to play. Only the fields
//! needed for that are read here, and unknown fields are ignored.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The file name of the registry inside the packages folder.
pub const REGISTRY_FILENAME: &str = "registry.json";

/// The file name of a state's package inside its state folder.
pub const PACKAGE_FILENAME: &str = "package.bmsp";

/// The default packages folder, next to the game. `BEETLE_PACKAGES_DIR` overrides it.
pub const DEFAULT_PACKAGES_DIR: &str = "packages";

/// One installed state that is the active state of its package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveState {
    pub id: String,
    pub state_hash: String,
    /// The state's `package.bmsp`.
    pub bmsp_path: PathBuf,
}

#[derive(Deserialize)]
struct RegistryFile {
    #[serde(default)]
    packages: BTreeMap<String, PackageRecord>,
}

#[derive(Deserialize)]
struct PackageRecord {
    id: String,
    active_state: String,
    #[serde(default)]
    state_hashes: BTreeMap<String, StateRecord>,
}

#[derive(Deserialize)]
struct StateRecord {
    path: String,
}

/// The packages folder for this process: `BEETLE_PACKAGES_DIR`, or `packages`.
pub fn packages_root() -> PathBuf {
    std::env::var("BEETLE_PACKAGES_DIR")
        .map_or_else(|_| PathBuf::from(DEFAULT_PACKAGES_DIR), PathBuf::from)
}

/// The active state of every package in `root`'s registry. A missing registry
/// means nothing is installed. A package whose active state has no record is
/// skipped, as the package manager skips it.
pub fn read_active_states(root: &Path) -> Result<Vec<ActiveState>, String> {
    let registry_path = root.join(REGISTRY_FILENAME);
    let text = match fs::read_to_string(&registry_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", registry_path.display())),
    };
    let registry: RegistryFile = serde_json::from_str(&text)
        .map_err(|e| format!("cannot parse {}: {e}", registry_path.display()))?;
    Ok(registry
        .packages
        .into_values()
        .filter_map(|record| {
            let state = record.state_hashes.get(&record.active_state)?;
            Some(ActiveState {
                id: record.id,
                state_hash: record.active_state,
                bmsp_path: root.join(&state.path).join(PACKAGE_FILENAME),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("bms-installed-{name}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_active_states_are_listed() {
        let root = temp_root("active");
        fs::write(
            root.join(REGISTRY_FILENAME),
            r#"{"packages":{
                "a":{"id":"a","name":"A","active_state":"s2",
                     "state_hashes":{"s1":{"path":"a/s1","installed_at":"x"},
                                     "s2":{"path":"a/s2","installed_at":"y"}}},
                "b":{"id":"b","name":"B","active_state":"zz",
                     "state_hashes":{"s9":{"path":"b/s9","installed_at":"z"}}}
            }}"#,
        )
        .unwrap();
        let states = read_active_states(&root).unwrap();
        assert_eq!(
            states,
            vec![ActiveState {
                id: "a".to_string(),
                state_hash: "s2".to_string(),
                bmsp_path: root.join("a/s2").join(PACKAGE_FILENAME),
            }]
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unknown_registry_fields_are_ignored() {
        let root = temp_root("unknown");
        fs::write(
            root.join(REGISTRY_FILENAME),
            r#"{"version":9,"packages":{"a":{"id":"a","name":"A","author":null,
                "active_state":"s","bga_status":"None","extra":[1,2],
                "state_hashes":{"s":{"path":"a/s","installed_at":"x","more":true}}}}}"#,
        )
        .unwrap();
        assert_eq!(read_active_states(&root).unwrap().len(), 1);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_registry_means_nothing_installed() {
        let root = temp_root("missing");
        assert_eq!(read_active_states(&root).unwrap(), Vec::new());
        fs::remove_dir_all(&root).ok();
    }
}
