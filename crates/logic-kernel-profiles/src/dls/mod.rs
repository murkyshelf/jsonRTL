//! Import profile for Sebastian Lague's Digital-Logic-Sim (DLS).
//!
//! A DLS project is a directory containing `ProjectDescription.json` and a
//! `Chips/` folder of hierarchical chip definitions. Every chip composes down
//! to the single combinational primitive **NAND**. This profile flattens each
//! chip to a NAND netlist and lowers it to a canonical circuit document.

use std::path::Path;

use crate::{NamedCircuit, ProfileError, ProjectConversion};

pub mod elaborate;
pub mod lower;
pub mod model;

/// The DLS import profile. See the module docs for the supported subset.
pub struct DlsProfile;

impl crate::Profile for DlsProfile {
    fn id(&self) -> &'static str {
        "dls"
    }

    fn detect(&self, path: &Path) -> bool {
        path.is_dir()
            && path.join("ProjectDescription.json").is_file()
            && path.join("Chips").is_dir()
    }

    fn convert(&self, path: &Path) -> Result<ProjectConversion, ProfileError> {
        let project = model::load_project(path)?;
        let mut circuits = Vec::with_capacity(project.chip_names.len());
        for name in &project.chip_names {
            let flat = elaborate::elaborate(&project, name)?;
            let document = lower::lower(name, &flat);
            circuits.push(NamedCircuit {
                name: name.clone(),
                document,
            });
        }
        Ok(ProjectConversion {
            project_name: project.name,
            circuits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Profile;
    use logic_kernel::{CompileOptions, Kernel};
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/dls")
            .join(name)
    }

    #[test]
    fn converts_every_chip_and_each_compiles() {
        let conversion = DlsProfile.convert(&fixture("test")).expect("convert");
        assert_eq!(conversion.project_name, "test");
        assert_eq!(conversion.circuits.len(), 5);
        for named in &conversion.circuits {
            let result =
                Kernel::default().compile_verilog(&named.document, &CompileOptions::default());
            assert!(
                result.has_output(),
                "chip '{}' failed: {:?}",
                named.name,
                result.diagnostics
            );
        }
    }

    #[test]
    fn detects_a_dls_project() {
        assert!(DlsProfile.detect(&fixture("test")));
        assert!(!DlsProfile.detect(&fixture("test").join("Chips")));
    }

    #[test]
    fn rejects_non_nand_builtin() {
        let error = elaborate::elaborate(
            &model::load_project(&fixture("unsupported")).expect("load"),
            "clocky",
        )
        .unwrap_err();
        match error {
            ProfileError::Unsupported { chip, detail } => {
                assert_eq!(chip, "clocky");
                assert!(detail.contains("CLOCK"), "{detail}");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn rejects_multi_bit_pin() {
        let error = elaborate::elaborate(
            &model::load_project(&fixture("unsupported")).expect("load"),
            "wide",
        )
        .unwrap_err();
        match error {
            ProfileError::Unsupported { chip, detail } => {
                assert_eq!(chip, "wide");
                assert!(detail.contains("bits wide"), "{detail}");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn convert_surfaces_unsupported_construct() {
        let error = DlsProfile.convert(&fixture("unsupported")).unwrap_err();
        assert!(matches!(error, ProfileError::Unsupported { .. }));
    }
}
