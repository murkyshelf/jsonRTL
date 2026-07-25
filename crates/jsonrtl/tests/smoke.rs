use jsonrtl::{CircuitDocument, CompileOptions, Kernel};

const MINIMAL_AND: &str = include_str!("../../../tests/fixtures/valid/minimal-and.json");
const MINIMAL_AND_VERILOG: &str = include_str!("../../../tests/golden/minimal-and.v");

#[test]
fn kernel_parse_validate_and_compile_smoke_test() {
    let document = CircuitDocument::from_json(MINIMAL_AND).expect("fixture parses");
    let kernel = Kernel::default();

    let validation = kernel.validate(&document);
    assert!(!validation.has_errors(), "{validation:#?}");

    let result = kernel.compile_verilog(&document, &CompileOptions::default());
    assert!(!result.diagnostics.has_errors(), "{result:#?}");
    assert_eq!(result.verilog.as_deref(), Some(MINIMAL_AND_VERILOG));
    assert!(result.source_map.is_some());
}
