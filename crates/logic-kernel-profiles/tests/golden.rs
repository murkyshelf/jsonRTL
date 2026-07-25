//! Byte-exact golden coverage: the bundled DLS project must convert and compile
//! to the committed Verilog. Regenerate goldens with:
//!
//! ```sh
//! logic-kernel import crates/logic-kernel-profiles/tests/fixtures/dls/test \
//!     --out crates/logic-kernel-profiles/tests/golden --force
//! ```

use std::path::PathBuf;

use logic_kernel::{CompileOptions, Kernel};
use logic_kernel_profiles::{Profile, dls::DlsProfile};

fn crate_path(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(sub)
}

#[test]
fn dls_test_project_matches_golden_verilog() {
    let conversion = DlsProfile
        .convert(&crate_path("tests/fixtures/dls/test"))
        .expect("convert DLS test project");

    assert_eq!(conversion.circuits.len(), 5);
    for named in &conversion.circuits {
        let result = Kernel::default().compile_verilog(&named.document, &CompileOptions::default());
        let verilog = result
            .verilog
            .unwrap_or_else(|| panic!("chip '{}' produced no Verilog", named.name));

        let golden_path = crate_path("tests/golden").join(format!("{}.v", named.name));
        let golden = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|_| panic!("missing golden file {}", golden_path.display()));

        assert_eq!(verilog, golden, "golden mismatch for chip '{}'", named.name);
    }
}
