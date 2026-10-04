//! Serde model for Sebastian Lague's Digital-Logic-Sim project format and a
//! loader that reads a project directory into memory.
//!
//! Only the fields needed for logical conversion are modeled; DLS-specific
//! layout (positions, colours, wire routing points, display config) is ignored.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{ProfileError, ProjectUnits, is_safe_unit_name, read_bounded};

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PROJECT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PROJECT_CHIPS: usize = 4096;

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

/// Reads project identity and validates its chip listing without opening chips.
pub(super) fn load_metadata(dir: &Path) -> Result<ProjectUnits, ProfileError> {
    let description_path = dir.join("ProjectDescription.json");
    let description_text = read_bounded(&description_path, "project metadata", MAX_FILE_BYTES)?;
    let description: ProjectDescription =
        serde_json::from_str(&description_text).map_err(|error| ProfileError::Parse {
            path: description_path.display().to_string(),
            message: error.to_string(),
        })?;

    if description.all_custom_chip_names.len() > MAX_PROJECT_CHIPS {
        return Err(ProfileError::Limit {
            chip: description.project_name,
            detail: format!("project lists more than {MAX_PROJECT_CHIPS} custom chips"),
        });
    }

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

    Ok(ProjectUnits {
        project_name: description.project_name,
        unit_names: description.all_custom_chip_names,
    })
}

/// Keeps loaded chip definitions across independent unit conversions. Only a
/// selected unit's dependency closure is read; unrelated files remain untouched.
pub(super) struct ProjectLoader {
    directory: PathBuf,
    pub(super) project: DlsProject,
    listed_names: BTreeSet<String>,
    heights: BTreeMap<String, usize>,
    source_bytes: u64,
    failures: BTreeMap<String, FileFailure>,
}

enum FileFailure {
    Io(std::io::Error),
    Parse(String),
    Limit(String),
}

impl FileFailure {
    fn error(&self, path: &Path, chip: &str) -> ProfileError {
        match self {
            Self::Io(source) => ProfileError::Io {
                path: path.display().to_string(),
                source: source
                    .raw_os_error()
                    .map(std::io::Error::from_raw_os_error)
                    .unwrap_or_else(|| std::io::Error::new(source.kind(), source.to_string())),
            },
            Self::Parse(message) => ProfileError::Parse {
                path: path.display().to_string(),
                message: message.clone(),
            },
            Self::Limit(detail) => ProfileError::Limit {
                chip: chip.to_string(),
                detail: detail.clone(),
            },
        }
    }
}

impl ProjectLoader {
    pub(super) fn new(dir: &Path) -> Result<Self, ProfileError> {
        let metadata = load_metadata(dir)?;
        let listed_names = metadata.unit_names.iter().cloned().collect();
        Ok(Self {
            directory: dir.to_owned(),
            project: DlsProject {
                name: metadata.project_name,
                chip_names: metadata.unit_names,
                chips: BTreeMap::new(),
            },
            listed_names,
            heights: BTreeMap::new(),
            source_bytes: 0,
            failures: BTreeMap::new(),
        })
    }

    fn load_chip(&mut self, name: &str) -> Result<(), ProfileError> {
        if self.project.chips.contains_key(name) {
            return Ok(());
        }
        let chip_path = self.directory.join("Chips").join(format!("{name}.json"));
        if let Some(failure) = self.failures.get(name) {
            return Err(failure.error(&chip_path, name));
        }
        let result = self.read_chip(name, &chip_path);
        match result {
            Ok(chip) => {
                self.project.chips.insert(name.to_string(), chip);
                Ok(())
            }
            Err(error) => {
                let failure = match error {
                    ProfileError::Io { source, .. } => FileFailure::Io(source),
                    ProfileError::Parse { message, .. } => FileFailure::Parse(message),
                    ProfileError::Limit { detail, .. } => FileFailure::Limit(detail),
                    other => return Err(other),
                };
                let error = failure.error(&chip_path, name);
                self.failures.insert(name.to_string(), failure);
                Err(error)
            }
        }
    }

    fn read_chip(&mut self, name: &str, chip_path: &Path) -> Result<ChipDef, ProfileError> {
        let remaining = MAX_PROJECT_BYTES - self.source_bytes;
        let chip_text =
            read_bounded(chip_path, name, MAX_FILE_BYTES.min(remaining)).map_err(|error| {
                match error {
                    ProfileError::Limit { .. } if remaining < MAX_FILE_BYTES => {
                        ProfileError::Limit {
                            chip: name.to_string(),
                            detail: format!(
                                "project chip sources exceed {MAX_PROJECT_BYTES} bytes"
                            ),
                        }
                    }
                    other => other,
                }
            })?;
        self.source_bytes += chip_text.len() as u64;
        serde_json::from_str(&chip_text).map_err(|error| ProfileError::Parse {
            path: chip_path.display().to_string(),
            message: error.to_string(),
        })
    }

    pub(super) fn load_unit(&mut self, unit: &str) -> Result<(), ProfileError> {
        if !self.listed_names.contains(unit) {
            return Err(ProfileError::UnknownUnit {
                unit: unit.to_string(),
            });
        }
        self.load_dependencies(unit, &mut Vec::new()).map(|_| ())
    }

    fn load_dependencies(
        &mut self,
        name: &str,
        stack: &mut Vec<String>,
    ) -> Result<usize, ProfileError> {
        let depth = stack.len() + 1;
        let cached_height = self.heights.get(name).copied();
        if depth + cached_height.unwrap_or(1) - 1 > super::elaborate::MAX_DEPTH {
            return Err(ProfileError::Structure {
                chip: name.to_string(),
                detail: format!(
                    "chip nesting exceeds depth limit {}",
                    super::elaborate::MAX_DEPTH
                ),
            });
        }
        if let Some(height) = cached_height {
            return Ok(height);
        }
        if stack.iter().any(|ancestor| ancestor == name) {
            return Err(ProfileError::Structure {
                chip: stack.last().cloned().unwrap_or_else(|| name.to_string()),
                detail: format!("chip reference cycle through '{name}'"),
            });
        }
        self.load_chip(name)?;
        // Names outside the project remain elaboration's unsupported-construct
        // diagnostics. Never build a path from them.
        let dependencies: Vec<_> = self.project.chips[name]
            .sub_chips
            .iter()
            .filter(|sub| {
                super::builtin::Builtin::parse(&sub.name).is_none()
                    && self.listed_names.contains(&sub.name)
            })
            .map(|sub| sub.name.clone())
            .collect();
        stack.push(name.to_string());
        let mut height = 1;
        for dependency in dependencies {
            height = height.max(self.load_dependencies(&dependency, stack)? + 1);
        }
        stack.pop();
        self.heights.insert(name.to_string(), height);
        Ok(height)
    }
}

/// Reads a DLS project directory into a [`DlsProject`].
///
/// Reads `ProjectDescription.json` for the project name and chip list, then
/// every `Chips/<Name>.json`, preserving the eager loader's public contract.
pub fn load_project(dir: &Path) -> Result<DlsProject, ProfileError> {
    let mut loader = ProjectLoader::new(dir)?;
    for name in loader.project.chip_names.clone() {
        loader.load_chip(&name)?;
    }
    Ok(loader.project)
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

    fn copy_fixture() -> PathBuf {
        let dir = project_listing("[]");
        std::fs::copy(
            fixture().join("ProjectDescription.json"),
            dir.join("ProjectDescription.json"),
        )
        .unwrap();
        for entry in std::fs::read_dir(fixture().join("Chips")).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.join("Chips").join(entry.file_name())).unwrap();
        }
        dir
    }

    #[test]
    fn shared_dependency_is_reused_for_the_next_unit() {
        let dir = copy_fixture();
        let mut loader = ProjectLoader::new(&dir).unwrap();
        loader.load_unit("AND").unwrap();
        std::fs::remove_file(dir.join("Chips/AND.json")).unwrap();
        let result = loader.load_unit("XOR");
        std::fs::remove_dir_all(&dir).unwrap();
        result.expect("XOR must reuse the AND definition already loaded");
        let flat = super::super::elaborate::elaborate(&loader.project, "XOR").unwrap();
        let document = super::super::lower::lower("XOR", &flat);
        let result = jsonrtl::Kernel::default()
            .compile_verilog(&document, &jsonrtl::CompileOptions::default());
        assert!(result.has_output(), "{:?}", result.diagnostics);
    }

    #[test]
    fn shared_malformed_dependency_is_not_re_read_for_the_next_unit() {
        let dir = copy_fixture();
        std::fs::write(dir.join("Chips/OR.json"), "{broken").unwrap();
        let mut loader = ProjectLoader::new(&dir).unwrap();
        assert!(matches!(
            loader.load_unit("XOR"),
            Err(ProfileError::Parse { .. })
        ));
        std::fs::remove_file(dir.join("Chips/OR.json")).unwrap();
        let result = loader.load_unit("1-bit adder");
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            matches!(result, Err(ProfileError::Parse { .. })),
            "cached parse error expected, got {result:?}"
        );
    }

    fn dependency_chain(length: usize, omit_tail: bool) -> PathBuf {
        let mut names: Vec<_> = (0..length).map(|index| format!("C{index}")).collect();
        names.push("Good".to_string());
        let dir = project_listing(&serde_json::to_string(&names).unwrap());
        for (index, name) in names.iter().enumerate() {
            if omit_tail && index == length - 1 {
                continue;
            }
            let sub_chips = if index + 1 < length {
                serde_json::json!([{ "Name": format!("C{}", index + 1), "ID": 1 }])
            } else {
                serde_json::json!([])
            };
            std::fs::write(
                dir.join("Chips").join(format!("{name}.json")),
                serde_json::to_vec(&serde_json::json!({
                    "Name": name,
                    "InputPins": [], "OutputPins": [], "SubChips": sub_chips, "Wires": [],
                }))
                .unwrap(),
            )
            .unwrap();
        }
        dir
    }

    #[test]
    fn too_deep_dependency_chain_is_rejected_before_reading_its_tail() {
        use crate::Profile;
        let dir = dependency_chain(257, true);
        let results = crate::dls::DlsProfile.convert_units(&dir, &["C0".into(), "Good".into()]);
        std::fs::remove_dir_all(&dir).unwrap();
        let results = results.unwrap();
        match &results[0] {
            Err(ProfileError::Structure { detail, .. }) => {
                assert!(detail.contains("depth"), "{detail}")
            }
            other => panic!("expected early depth rejection, got {other:?}"),
        }
        assert_eq!(
            results[1].as_ref().expect("good sibling converts").circuits[0].name,
            "Good"
        );
    }

    #[test]
    fn dependency_chain_at_the_depth_limit_still_converts() {
        use crate::Profile;
        let dir = dependency_chain(256, false);
        let result = crate::dls::DlsProfile.convert_unit(&dir, "C0");
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            result.expect("depth 256 stays supported").circuits[0].name,
            "C0"
        );
    }

    #[test]
    fn a_dependency_reached_shallow_first_is_checked_on_a_deeper_path() {
        let dir = dependency_chain(253, false);
        let description_path = dir.join("ProjectDescription.json");
        let mut description: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&description_path).unwrap()).unwrap();
        let names = description["AllCustomChipNames"].as_array_mut().unwrap();
        names.extend(["Root", "Shared", "Middle", "Leaf"].map(|name| serde_json::json!(name)));
        std::fs::write(description_path, serde_json::to_vec(&description).unwrap()).unwrap();
        for (name, children) in [
            ("Root", ["Shared", "C0"].as_slice()),
            ("C252", ["Shared"].as_slice()),
            ("Shared", ["Middle"].as_slice()),
            ("Middle", ["Leaf"].as_slice()),
            ("Leaf", [].as_slice()),
        ] {
            let sub_chips: Vec<_> = children
                .iter()
                .enumerate()
                .map(|(index, child)| serde_json::json!({"Name":child,"ID":index + 1}))
                .collect();
            std::fs::write(dir.join("Chips").join(format!("{name}.json")), serde_json::to_vec(&serde_json::json!({
                "Name": name, "InputPins": [], "OutputPins": [], "SubChips": sub_chips, "Wires": [],
            })).unwrap()).unwrap();
        }
        let mut loader = ProjectLoader::new(&dir).unwrap();
        // The short use is valid and warms the shared closure's cache.
        loader.load_unit("Shared").unwrap();
        let result = loader.load_unit("Root");
        std::fs::remove_dir_all(&dir).unwrap();
        match result {
            Err(ProfileError::Structure { detail, .. }) => {
                assert!(detail.contains("depth"), "{detail}")
            }
            other => panic!("expected shared DAG depth rejection, got {other:?}"),
        }
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
    fn rejects_oversized_project_description_before_parsing() {
        let dir = project_listing("[]");
        std::fs::OpenOptions::new()
            .write(true)
            .open(dir.join("ProjectDescription.json"))
            .unwrap()
            .set_len(8 * 1024 * 1024 + 1)
            .unwrap();
        let result = load_project(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            matches!(result, Err(ProfileError::Limit { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn rejects_excessive_chip_listing_before_opening_chip_files() {
        let names: Vec<_> = (0..4097).map(|index| format!("C{index}")).collect();
        let dir = project_listing(&serde_json::to_string(&names).unwrap());
        let result = load_project(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            matches!(result, Err(ProfileError::Limit { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn rejects_oversized_chip_file_before_parsing() {
        let dir = project_listing(r#"["C"]"#);
        std::fs::File::create(dir.join("Chips/C.json"))
            .unwrap()
            .set_len(8 * 1024 * 1024 + 1)
            .unwrap();
        let result = load_project(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            matches!(result, Err(ProfileError::Limit { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn rejects_project_sources_exceeding_the_total_byte_budget() {
        let names: Vec<_> = (0..9).map(|index| format!("C{index}")).collect();
        let dir = project_listing(&serde_json::to_string(&names).unwrap());
        let mut text =
            br#"{"Name":"C","InputPins":[],"OutputPins":[],"SubChips":[],"Wires":[]}"#.to_vec();
        text.resize(8 * 1024 * 1024, b' ');
        for name in names {
            std::fs::write(dir.join("Chips").join(format!("{name}.json")), &text).unwrap();
        }
        let result = load_project(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            matches!(result, Err(ProfileError::Limit { .. })),
            "{result:?}"
        );
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
