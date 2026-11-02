//! Which song a chart-only package plays with (B20,
//! docs/plans/2026-11-02-chart-only-packages-plan.md 3.1 and 3.2).
//!
//! A package whose charts borrow their key sounds from a song elsewhere says
//! so in its manifest: `"base": {"charts": [<chart ids>], "package": <id>}`.
//! Chart ids are fixed by the chart bytes, so the record means the same on
//! any machine. The scan turns each record into a group for the grouping's
//! rule 0 (the package's charts with the base charts), so the package joins
//! that song without the borrowed key sound guess (`grouping` rule 2).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use beetle_core::ChartId;
use bms_package::installed::read_active_states;
use bms_package::{Manifest, MANIFEST_FILENAME};

/// The manifest field.
pub const FIELD: &str = "base";

/// A chart-only package's base.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Base {
    /// Charts of the song it plays with: it joins the song holding any.
    pub charts: Vec<ChartId>,
    /// The package holding that song's key sounds, when one does (for notices).
    pub package: Option<String>,
}

impl Base {
    /// The base a manifest records. `None` without one, or with no chart
    /// that can be read.
    pub fn of(manifest: &Manifest) -> Option<Self> {
        let value = manifest.extra.get(FIELD)?;
        let charts: Vec<ChartId> = value
            .get("charts")?
            .as_array()?
            .iter()
            .filter_map(|c| ChartId::from_hex(c.as_str()?))
            .collect();
        if charts.is_empty() {
            return None;
        }
        let package = value
            .get("package")
            .and_then(|p| p.as_str())
            .map(str::to_string);
        Some(Self { charts, package })
    }

    /// Records the base in `manifest`, charts sorted and unique so the same
    /// base always writes the same bytes (INV-6).
    pub fn write_to(&self, manifest: &mut Manifest) {
        let charts: BTreeSet<String> = self.charts.iter().map(ChartId::to_hex).collect();
        let mut value = serde_json::json!({ "charts": charts.into_iter().collect::<Vec<_>>() });
        if let Some(package) = &self.package {
            value["package"] = serde_json::Value::String(package.clone());
        }
        manifest.extra.insert(FIELD.to_string(), value);
    }
}

/// The base charts of each active package state under the packages folder
/// `root`, by source (`<id>@<state hash>`), from the `manifest.json` each
/// state keeps beside its package.
pub fn of_active(root: &Path) -> BTreeMap<String, Vec<ChartId>> {
    let Ok(states) = read_active_states(root) else {
        return BTreeMap::new();
    };
    states
        .into_iter()
        .filter_map(|state| {
            let text =
                fs::read_to_string(state.bmsp_path.with_file_name(MANIFEST_FILENAME)).ok()?;
            let base = Base::of(&Manifest::from_json_str(&text).ok()?)?;
            Some((format!("{}@{}", state.id, state.state_hash), base.charts))
        })
        .collect()
}

/// Groups for the grouping's rule 0: each based package's charts (`copies`
/// are a source and a chart there) with its base charts.
pub fn links<'a>(
    bases: &BTreeMap<String, Vec<ChartId>>,
    copies: impl IntoIterator<Item = (&'a str, ChartId)>,
) -> Vec<Vec<ChartId>> {
    let mut own: BTreeMap<&str, BTreeSet<ChartId>> = BTreeMap::new();
    for (source, chart) in copies {
        if bases.contains_key(source) {
            own.entry(source).or_default().insert(chart);
        }
    }
    own.into_iter()
        .map(|(source, charts)| {
            let mut group: Vec<ChartId> = charts.into_iter().collect();
            group.extend(bases[source].iter().copied());
            group
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base_survives_the_manifest_and_makes_a_link() {
        let id = |n: u8| ChartId::of_bytes(&[n]);
        let mut manifest = Manifest::new("t.charts", "T");
        assert_eq!(Base::of(&manifest), None);
        let base = Base {
            charts: vec![id(2), id(1), id(2)],
            package: Some("t.song".into()),
        };
        base.write_to(&mut manifest);
        let back = Manifest::from_json_str(&manifest.to_json_string().unwrap()).unwrap();
        let read = Base::of(&back).unwrap();
        let mut sorted = vec![id(1), id(2)];
        sorted.sort_by_key(ChartId::to_hex);
        assert_eq!(read.charts, sorted);
        assert_eq!(read.package.as_deref(), Some("t.song"));

        let bases = BTreeMap::from([("t.charts@h".to_string(), vec![id(1)])]);
        let copies = [
            ("t.charts@h", id(7)),
            ("t.charts@h", id(8)),
            ("other@h", id(9)),
        ];
        let groups = links(&bases, copies);
        assert_eq!(groups.len(), 1);
        let got: BTreeSet<ChartId> = groups[0].iter().copied().collect();
        assert_eq!(got, BTreeSet::from([id(7), id(8), id(1)]));
    }
}
