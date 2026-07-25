//! Serde model for Sebastian Lague's Digital-Logic-Sim project format and a
//! loader that reads a project directory into memory.
//!
//! Only the fields needed for logical conversion are modeled; DLS-specific
//! layout (positions, colours, wire routing points, display config) is ignored.

use std::{collections::BTreeMap, path::Path};

use serde::Deserialize;

use crate::{ProfileError, is_safe_unit_name};

/// A loaded DLS project: its name and every custom chip, keyed by chip name.
#[derive(Debug, Clone, PartialEq)]
pub struct DlsProject {
    pub name: String,
    pub chip_names: Vec<String>,
    pub chips: BTreeMap<String, ChipDef>,
}

/// One custom chip definition (a `Chips/<Name>.json` file).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ChipDef {
    pub name: String,
    pub input_pins: Vec<PinDef>,
    pub output_pins: Vec<PinDef>,
    pub sub_chips: Vec<SubChip>,
    pub wires: Vec<Wire>,
}

/// A boundary pin (module input or output).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PinDef {
    pub name: String,
    #[serde(rename = "ID")]
    pub id: i64,
    pub bit_count: u32,
}

/// An instance of another chip placed inside this chip.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SubChip {
    /// The referenced chip type (built-in name like `NAND`, or a custom name).
    pub name: String,
    #[serde(rename = "ID")]
    pub id: i64,
}

/// A directed connection between two pin endpoints.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Wire {
    #[serde(rename = "SourcePinAddress")]
    pub source: PinAddress,
    #[serde(rename = "TargetPinAddress")]
    pub target: PinAddress,
}

/// Addresses one pin: which pin of which owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct PinAddress {
    #[serde(rename = "PinID")]
    pub pin_id: i64,
    #[serde(rename = "PinOwnerID")]
    pub pin_owner_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ProjectDescription {
    project_name: String,
    all_custom_chip_names: Vec<String>,
}

/// Reads a DLS project directory into a [`DlsProject`].
///
/// Reads `ProjectDescription.json` for the project name and chip list, then
/// each `Chips/<Name>.json`.
pub fn load_project(dir: &Path) -> Result<DlsProject, ProfileError> {
    let description_path = dir.join("ProjectDescription.json");
    let description_text =
        std::fs::read_to_string(&description_path).map_err(|source| ProfileError::Io {
            path: description_path.display().to_string(),
            source,
        })?;
    let description: ProjectDescription =
        serde_json::from_str(&description_text).map_err(|error| ProfileError::Parse {
            path: description_path.display().to_string(),
            message: error.to_string(),
        })?;

    // Chip names come from an untrusted file and are joined onto both input and
    // output directories, so they are validated before any path is built.
    let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
    for chip_name in &description.all_custom_chip_names {
        if !is_safe_unit_name(chip_name) {
            return Err(ProfileError::Structure {
                chip: chip_name.clone(),
                detail:
                    "chip name is not a single ordinary path component; it could escape the project directory"
                        .into(),
            });
        }
        if seen.insert(chip_name.as_str(), ()).is_some() {
            return Err(ProfileError::Structure {
                chip: chip_name.clone(),
                detail: "chip name is listed more than once in AllCustomChipNames".into(),
            });
        }
    }

    let mut chips = BTreeMap::new();
    for chip_name in &description.all_custom_chip_names {
        let chip_path = dir.join("Chips").join(format!("{chip_name}.json"));
        let chip_text = std::fs::read_to_string(&chip_path).map_err(|source| ProfileError::Io {
            path: chip_path.display().to_string(),
            source,
        })?;
        let chip: ChipDef =
            serde_json::from_str(&chip_text).map_err(|error| ProfileError::Parse {
                path: chip_path.display().to_string(),
                message: error.to_string(),
            })?;
        chips.insert(chip_name.clone(), chip);
    }

    Ok(DlsProject {
        name: description.project_name,
        chip_names: description.all_custom_chip_names,
        chips,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dls/test")
    }

    #[test]
    fn loads_project_metadata() {
        let project = load_project(&fixture()).expect("load");
        assert_eq!(project.name, "test");
        assert_eq!(project.chips.len(), 5);
        assert!(project.chips.contains_key("1-bit adder"));
    }

    /// Writes a throwaway project whose description lists `names`.
    fn project_listing(names: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "jsonrtl-dls-model-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("Chips")).unwrap();
        std::fs::write(
            dir.join("ProjectDescription.json"),
            format!("{{\"ProjectName\":\"p\",\"AllCustomChipNames\":{names}}}"),
        )
        .unwrap();
        dir
    }

    #[test]
    fn rejects_chip_names_that_escape_the_project_directory() {
        // Regression: a traversing name was joined straight onto Chips/ and the
        // output directory, letting a project read and write outside both.
        let dir = project_listing(r#"["../escaped"]"#);
        let error = load_project(&dir).unwrap_err();
        match error {
            ProfileError::Structure { chip, detail } => {
                assert_eq!(chip, "../escaped");
                assert!(detail.contains("escape"), "{detail}");
            }
            other => panic!("expected Structure, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_duplicate_chip_names() {
        // Regression: duplicates surfaced later as a misleading
        // "file already exists; pass --force" i/o error.
        let dir = project_listing(r#"["F","F"]"#);
        let error = load_project(&dir).unwrap_err();
        match error {
            ProfileError::Structure { chip, detail } => {
                assert_eq!(chip, "F");
                assert!(detail.contains("more than once"), "{detail}");
            }
            other => panic!("expected Structure, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn and_chip_has_expected_shape() {
        let project = load_project(&fixture()).expect("load");
        let and = project.chips.get("AND").expect("AND present");
        assert_eq!(and.input_pins.len(), 2);
        assert_eq!(and.output_pins.len(), 1);
        assert_eq!(and.sub_chips.len(), 2);
        assert_eq!(and.wires.len(), 5);
        assert!(and.sub_chips.iter().all(|sub| sub.name == "NAND"));
        assert!(and.input_pins.iter().all(|pin| pin.bit_count == 1));
    }
}
