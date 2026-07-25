//! Import profile for Sebastian Lague's Digital-Logic-Sim (DLS).
//!
//! A DLS project is a directory containing `ProjectDescription.json` and a
//! `Chips/` folder of hierarchical chip definitions. Every chip composes down
//! to the single combinational primitive **NAND**. This profile flattens each
//! chip to a NAND netlist and lowers it to a canonical circuit document.

use std::path::Path;

use crate::{NamedCircuit, ProfileError, ProfileStatus, ProjectConversion, ProjectUnits};

pub mod builtin;
pub mod elaborate;
pub mod lower;
pub mod model;

/// The DLS import profile. See the module docs for the supported subset.
pub struct DlsProfile;

impl crate::Profile for DlsProfile {
    fn id(&self) -> &'static str {
        "dls"
    }

    fn source(&self) -> &'static str {
        "Sebastian Lague's Digital-Logic-Sim"
    }

    fn input_hint(&self) -> &'static str {
        "a project directory (ProjectDescription.json + Chips/)"
    }

    fn supports(&self) -> &'static str {
        "combinational logic of any width; NAND, bus split/merge, BUS-N"
    }

    fn status(&self) -> ProfileStatus {
        ProfileStatus::Stable
    }

    fn detect(&self, path: &Path) -> bool {
        path.is_dir()
            && path.join("ProjectDescription.json").is_file()
            && path.join("Chips").is_dir()
    }

    fn units(&self, path: &Path) -> Result<ProjectUnits, ProfileError> {
        let project = model::load_project(path)?;
        Ok(ProjectUnits {
            project_name: project.name,
            unit_names: project.chip_names,
        })
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

    fn convert_unit(&self, path: &Path, unit: &str) -> Result<ProjectConversion, ProfileError> {
        let project = model::load_project(path)?;
        if !project.chip_names.iter().any(|name| name == unit) {
            return Err(ProfileError::UnknownUnit {
                unit: unit.to_string(),
            });
        }
        // Elaboration walks only this chip's dependency closure, so an
        // unsupported chip elsewhere in the project cannot fail this call.
        let flat = elaborate::elaborate(&project, unit)?;
        Ok(ProjectConversion {
            project_name: project.name,
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
    fn multi_bit_pins_keep_their_width_through_to_verilog() {
        // `wide` is a 4-bit input wired straight to a 4-bit output. Before
        // Phase M this was rejected outright.
        let conversion = DlsProfile
            .convert_unit(&fixture("unsupported"), "wide")
            .expect("a multi-bit chip converts");
        let document = &conversion.circuits[0].document;
        assert_eq!(document.schema_version.as_str(), "1.1");
        assert!(
            document.circuit.ports.iter().all(|port| port.width == 4),
            "ports should stay 4 bits wide"
        );
        let verilog = Kernel::default()
            .compile_verilog(document, &CompileOptions::default())
            .verilog
            .expect("compiles");
        assert!(verilog.contains("input wire [3:0] in;"), "{verilog}");
        assert!(verilog.contains("output wire [3:0] out;"), "{verilog}");
    }

    #[test]
    fn convert_surfaces_unsupported_construct() {
        let error = DlsProfile.convert(&fixture("unsupported")).unwrap_err();
        assert!(matches!(error, ProfileError::Unsupported { .. }));
    }

    #[test]
    fn one_unsupported_chip_does_not_block_a_supported_sibling() {
        // Whole-project conversion fails: `clocky` and `wide` are unsupported.
        assert!(DlsProfile.convert(&fixture("unsupported")).is_err());

        // Asking for a supported chip in the same project still succeeds.
        let conversion = DlsProfile
            .convert_unit(&fixture("unsupported"), "inverter")
            .expect("supported chip converts despite unsupported siblings");
        assert_eq!(conversion.project_name, "unsupported");
        assert_eq!(conversion.circuits.len(), 1);
        assert_eq!(conversion.circuits[0].name, "inverter");
        let result = Kernel::default()
            .compile_verilog(&conversion.circuits[0].document, &CompileOptions::default());
        assert!(result.has_output(), "{:?}", result.diagnostics);
    }

    #[test]
    fn convert_unit_reports_an_unknown_chip() {
        let error = DlsProfile
            .convert_unit(&fixture("unsupported"), "nope")
            .unwrap_err();
        match error {
            ProfileError::UnknownUnit { unit } => assert_eq!(unit, "nope"),
            other => panic!("expected UnknownUnit, got {other:?}"),
        }
    }

    #[test]
    fn convert_unit_still_fails_on_the_chip_that_was_asked_for() {
        let error = DlsProfile
            .convert_unit(&fixture("unsupported"), "clocky")
            .unwrap_err();
        match error {
            ProfileError::Unsupported { chip, .. } => assert_eq!(chip, "clocky"),
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }
}
