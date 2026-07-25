//! Import profile for Logisim and Logisim Evolution `.circ` projects.
//!
//! Both tools share the `.circ` XML format, so one profile handles them and can
//! report which variant produced the file. Unlike DLS, Logisim connectivity is
//! **geometric**: wires attach to whatever port shares their coordinate, so
//! conversion recomputes port positions from each component's anchor, facing,
//! and size. See [`geometry`] for those rules and their calibration status.

use std::path::Path;

use crate::{NamedCircuit, ProfileError, ProjectConversion};

pub mod elaborate;
pub mod geometry;
pub mod lower;
pub mod model;

/// The Logisim / Logisim Evolution import profile.
pub struct LogisimProfile;

/// A `.circ` file holds the whole project, so its stem names the project.
fn project_name(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "logisim".to_string())
}

impl crate::Profile for LogisimProfile {
    fn id(&self) -> &'static str {
        "logisim"
    }

    fn detect(&self, path: &Path) -> bool {
        path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("circ"))
    }

    fn convert(&self, path: &Path) -> Result<ProjectConversion, ProfileError> {
        let project = model::load_project(path)?;
        let mut circuits = Vec::with_capacity(project.circuit_names.len());
        for name in &project.circuit_names {
            let flat = elaborate::elaborate(&project, name)?;
            circuits.push(NamedCircuit {
                name: name.clone(),
                document: lower::lower(name, &flat),
            });
        }
        Ok(ProjectConversion {
            project_name: project_name(path),
            circuits,
        })
    }

    fn convert_unit(&self, path: &Path, unit: &str) -> Result<ProjectConversion, ProfileError> {
        let project = model::load_project(path)?;
        if !project.circuit_names.iter().any(|name| name == unit) {
            return Err(ProfileError::UnknownUnit {
                unit: unit.to_string(),
            });
        }
        let flat = elaborate::elaborate(&project, unit)?;
        Ok(ProjectConversion {
            project_name: project_name(path),
            circuits: vec![NamedCircuit {
                name: unit.to_string(),
                document: lower::lower(unit, &flat),
            }],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Profile;
    use jsonrtl::{CompileOptions, Kernel};

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/logisim")
            .join(name)
    }

    #[test]
    fn detects_circ_files_only() {
        assert!(LogisimProfile.detect(&fixture("basic.circ")));
        assert!(!LogisimProfile.detect(fixture("basic.circ").parent().unwrap()));
    }

    #[test]
    fn converts_and_compiles_the_sample_project() {
        let conversion = LogisimProfile
            .convert(&fixture("basic.circ"))
            .expect("convert");
        assert_eq!(conversion.project_name, "basic");
        assert!(!conversion.circuits.is_empty());
        for named in &conversion.circuits {
            let result =
                Kernel::default().compile_verilog(&named.document, &CompileOptions::default());
            assert!(
                result.has_output(),
                "circuit '{}' failed: {:?}",
                named.name,
                result.diagnostics
            );
        }
    }
}
