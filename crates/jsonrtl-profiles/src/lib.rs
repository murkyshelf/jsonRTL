//! Foreign-format import profiles for `jsonrtl`.
//!
//! A [`Profile`] converts a third-party digital-logic project (a directory on
//! disk) into one or more canonical [`CircuitDocument`]s that the kernel can
//! validate and compile to Verilog. Profiles depend only on the public
//! `jsonrtl` contract; the core library has no knowledge of them.

use std::path::Path;

use jsonrtl::CircuitDocument;

pub mod dls;
pub mod logisim;

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

    /// Conversion would exceed a profile resource bound. Reported before the
    /// work is performed so a hostile or runaway project cannot exhaust memory.
    #[error("resource limit exceeded in chip '{chip}': {detail}")]
    Limit { chip: String, detail: String },

    /// A caller asked for a unit the project does not contain.
    #[error("no unit named '{unit}' in project")]
    UnknownUnit { unit: String },
}

/// Returns true when `name` is safe to use as a single file-name component.
///
/// Unit names come from untrusted project files and are joined onto both input
/// and output directories. A name is accepted only when it is exactly one
/// ordinary path component, so `..`, `a/b`, absolute paths, and Windows drive
/// or separator forms can never escape the directory they are joined to.
#[must_use]
pub fn is_safe_unit_name(name: &str) -> bool {
    if name.is_empty()
        || name.contains('\\')
        || name.contains(':')
        || name.contains('\0')
        || name.contains('/')
    {
        return false;
    }
    let mut components = std::path::Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(part)), None) => part == std::ffi::OsStr::new(name),
        _ => false,
    }
}

/// A converter from one foreign project format to canonical circuit documents.
pub trait Profile {
    /// Stable identifier used on the CLI (e.g. `"dls"`).
    fn id(&self) -> &'static str;

    /// Returns true if `path` looks like a project this profile can convert.
    fn detect(&self, path: &Path) -> bool;

    /// Converts a project directory into canonical documents.
    fn convert(&self, path: &Path) -> Result<ProjectConversion, ProfileError>;

    /// Converts exactly one unit and the units it depends on, returning a
    /// conversion holding just that circuit.
    ///
    /// A unit the caller did not ask for cannot fail this call. Real projects
    /// accumulate chips that use constructs outside the supported subset, and
    /// one of them must not make every other chip unreachable.
    fn convert_unit(&self, path: &Path, unit: &str) -> Result<ProjectConversion, ProfileError>;
}

/// Every profile known to this build, in stable order.
#[must_use]
pub fn registry() -> Vec<Box<dyn Profile>> {
    vec![Box::new(dls::DlsProfile), Box::new(logisim::LogisimProfile)]
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
    fn unit_names_that_escape_a_directory_are_rejected() {
        // Ordinary DLS chip names stay usable.
        assert!(is_safe_unit_name("AND"));
        assert!(is_safe_unit_name("1-bit adder"));
        assert!(is_safe_unit_name("chip.v2"));

        // Anything that could escape the directory it is joined to.
        for hostile in [
            "",
            ".",
            "..",
            "../escaped",
            "../../etc/passwd",
            "a/b",
            "/absolute",
            "sub/../../x",
            "C:\\windows",
            "back\\slash",
            "nul\0byte",
        ] {
            assert!(
                !is_safe_unit_name(hostile),
                "accepted hostile name {hostile:?}"
            );
        }
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
