//! Foreign-format import profiles for `logic-kernel`.
//!
//! A [`Profile`] converts a third-party digital-logic project (a directory on
//! disk) into one or more canonical [`CircuitDocument`]s that the kernel can
//! validate and compile to Verilog. Profiles depend only on the public
//! `logic-kernel` contract; the core library has no knowledge of them.

use std::path::Path;

use logic_kernel::CircuitDocument;

pub mod dls;

/// One canonical circuit produced from a foreign project, named after its
/// source unit (for DLS, the chip name).
#[derive(Debug, Clone, PartialEq)]
pub struct NamedCircuit {
    pub name: String,
    pub document: CircuitDocument,
}

/// The full result of converting a foreign project directory.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectConversion {
    pub project_name: String,
    pub circuits: Vec<NamedCircuit>,
}

/// A failure while importing a foreign project.
///
/// These are *conversion* diagnostics, distinct from kernel validation
/// diagnostics: they describe why a foreign document could not be mapped onto
/// the canonical model, always naming the offending source unit where known.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    /// A file could not be read or a directory did not exist.
    #[error("i/o error reading '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// A foreign file was present but not valid for its format.
    #[error("could not parse '{path}': {message}")]
    Parse { path: String, message: String },

    /// The project uses a construct outside this profile's supported subset.
    #[error("unsupported construct in chip '{chip}': {detail}")]
    Unsupported { chip: String, detail: String },

    /// The project is structurally inconsistent (dangling reference, cycle,
    /// excessive depth, missing/duplicate driver, ...).
    #[error("structural error in chip '{chip}': {detail}")]
    Structure { chip: String, detail: String },
}

/// A converter from one foreign project format to canonical circuit documents.
pub trait Profile {
    /// Stable identifier used on the CLI (e.g. `"dls"`).
    fn id(&self) -> &'static str;

    /// Returns true if `path` looks like a project this profile can convert.
    fn detect(&self, path: &Path) -> bool;

    /// Converts a project directory into canonical documents.
    fn convert(&self, path: &Path) -> Result<ProjectConversion, ProfileError>;
}

/// Every profile known to this build, in stable order.
#[must_use]
pub fn registry() -> Vec<Box<dyn Profile>> {
    vec![Box::new(dls::DlsProfile)]
}

/// Returns the registered profile with the given id, if any.
#[must_use]
pub fn profile_by_id(id: &str) -> Option<Box<dyn Profile>> {
    registry().into_iter().find(|profile| profile.id() == id)
}

/// Auto-detects which registered profile can convert `path`.
#[must_use]
pub fn detect_profile(path: &Path) -> Option<Box<dyn Profile>> {
    registry().into_iter().find(|profile| profile.detect(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_exposes_the_dls_profile() {
        let ids: Vec<&str> = registry().iter().map(|profile| profile.id()).collect();
        assert!(ids.contains(&"dls"), "registry ids were {ids:?}");
    }

    #[test]
    fn profile_by_id_finds_dls() {
        assert!(profile_by_id("dls").is_some());
        assert!(profile_by_id("nope").is_none());
    }

    #[test]
    fn error_display_names_the_source_unit() {
        let error = ProfileError::Unsupported {
            chip: "1-bit adder".into(),
            detail: "CLOCK is not supported".into(),
        };
        let rendered = error.to_string();
        assert!(rendered.contains("1-bit adder"), "{rendered}");
        assert!(rendered.contains("CLOCK"), "{rendered}");
    }
}
