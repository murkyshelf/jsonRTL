use std::{collections::BTreeSet, panic};

use jsonrtl::{
    CircuitDocument, CompileOptions, DiagnosticCode, Kernel, SourceMapKind, VerilogIdentifier,
};

const MINIMAL_AND: &str = include_str!("../../../tests/fixtures/valid/minimal-and.json");
const HALF_ADDER: &str = include_str!("../../../tests/fixtures/valid/half-adder.json");
const FULL_ADDER: &str = include_str!("../../../tests/fixtures/valid/full-adder.json");
const EIGHT_BIT: &str = include_str!("../../../tests/fixtures/valid/eight-bit.json");
const CONST_EXAMPLE: &str = include_str!("../../../tests/fixtures/valid/const.json");
const EVERY_GATE: &str = include_str!("../../../tests/fixtures/valid/every-gate.json");
const SLICED_BUS: &str = include_str!("../../../tests/fixtures/valid/sliced-bus.json");
const SANITIZED_COLLISIONS: &str =
    include_str!("../../../tests/fixtures/valid/sanitized-collisions.json");
const COMBINED_INVALID: &str =
    include_str!("../../../tests/fixtures/semantic/combined-invalid.json");

const GOLDEN_CASES: [(&str, &str, &str); 8] = [
    (
        "minimal AND",
        MINIMAL_AND,
        include_str!("../../../tests/golden/minimal-and.v"),
    ),
    (
        "half adder",
        HALF_ADDER,
        include_str!("../../../tests/golden/half-adder.v"),
    ),
    (
        "full adder",
        FULL_ADDER,
        include_str!("../../../tests/golden/full-adder.v"),
    ),
    (
        "8-bit circuit",
        EIGHT_BIT,
        include_str!("../../../tests/golden/eight-bit.v"),
    ),
    (
        "sliced bus",
        SLICED_BUS,
        include_str!("../../../tests/golden/sliced-bus.v"),
    ),
    (
        "CONST",
        CONST_EXAMPLE,
        include_str!("../../../tests/golden/const.v"),
    ),
    (
        "every V1 gate",
        EVERY_GATE,
        include_str!("../../../tests/golden/every-gate.v"),
    ),
    (
        "sanitized collisions",
        SANITIZED_COLLISIONS,
        include_str!("../../../tests/golden/sanitized-collisions.v"),
    ),
];

#[test]
fn valid_examples_match_byte_exact_goldens() {
    for (name, input, expected) in GOLDEN_CASES {
        let result = compile(input, CompileOptions::default());
        assert!(!result.diagnostics.has_errors(), "{name}: {result:#?}");
        assert_eq!(result.verilog.as_deref(), Some(expected), "{name}");
        assert!(result.source_map.is_some(), "{name}");
    }
}

#[test]
fn every_v1_component_has_a_synthesizable_expression() {
    let output = compile(EVERY_GATE, CompileOptions::default())
        .verilog
        .expect("every-gate fixture compiles");
    for expression in [
        "a_net & b_net",
        "a_net",
        "1'b1",
        "~(a_net & b_net)",
        "~(a_net | b_net)",
        "~a_net",
        "a_net | b_net",
        "~(a_net ^ b_net)",
        "a_net ^ b_net",
    ] {
        assert!(
            output.contains(expression),
            "missing expression {expression}"
        );
    }
}

#[test]
fn invalid_circuits_never_return_partial_output() {
    let document = CircuitDocument::from_json(COMBINED_INVALID).expect("fixture is schema-valid");
    let result = Kernel::default().compile_verilog(&document, &CompileOptions::default());
    assert!(result.diagnostics.has_errors());
    assert!(!result.has_output());
    assert!(result.verilog.is_none());
    assert!(result.source_map.is_none());
}

#[test]
fn warnings_are_preserved_without_blocking_compilation() {
    let result = compile(SANITIZED_COLLISIONS, CompileOptions::default());
    assert!(!result.diagnostics.has_errors());
    assert!(result.has_output());
    assert!(result.diagnostics.diagnostics().iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            DiagnosticCode::NameRequiresSanitization
                | DiagnosticCode::NameVerilogKeyword
                | DiagnosticCode::NameSanitizationCollision
        )
    }));
}

#[test]
fn array_reordering_and_editor_metadata_do_not_change_generated_bytes() {
    let mut document = CircuitDocument::from_json(FULL_ADDER).expect("fixture parses");
    let expected = Kernel::default().compile_verilog(&document, &CompileOptions::default());

    document.circuit.ports.reverse();
    document.circuit.components.reverse();
    document.circuit.nets.reverse();
    document.editor_metadata = Some(jsonrtl::EditorMetadata(serde_json::json!({
        "positions": [8, 5, 3],
        "note": "must never reach normalized IR"
    })));

    let reordered = Kernel::default().compile_verilog(&document, &CompileOptions::default());
    assert_eq!(reordered.verilog, expected.verilog);
    assert_eq!(reordered.source_map, expected.source_map);
    assert_eq!(reordered.diagnostics, expected.diagnostics);
}

#[test]
fn identifier_policy_is_safe_total_and_deserialization_defensive() {
    for (raw, expected) in [
        ("plain_name", "plain_name"),
        ("123 name", "n_123_name"),
        ("data-in", "data_in"),
        ("Ω", "_"),
        ("", "unnamed"),
        ("module", "module_id"),
        ("output", "output_id"),
    ] {
        let identifier = VerilogIdentifier::from_untrusted(raw);
        assert_eq!(identifier.as_str(), expected);
        assert!(identifier.is_safe());
        let round_trip: VerilogIdentifier = serde_json::from_str(
            &serde_json::to_string(&identifier).expect("safe identifier serializes"),
        )
        .expect("serialized safe identifier deserializes");
        assert_eq!(round_trip, identifier);
    }

    for unsafe_json in [r#"""#, r#""12bad""#, r#""bad-name""#, r#""module""#] {
        assert!(serde_json::from_str::<VerilogIdentifier>(unsafe_json).is_err());
    }
}

#[test]
fn sanitization_is_collision_free_and_raw_names_do_not_leak() {
    let output = compile(SANITIZED_COLLISIONS, CompileOptions::default())
        .verilog
        .expect("warning-only fixture compiles");
    for raw in [
        "123 Ω module",
        "data-in",
        "data in",
        "and gate",
        "sanitized-collisions",
        "p-a",
        "n-a",
        "c-and",
    ] {
        assert!(!output.contains(raw), "raw name or stable ID leaked: {raw}");
    }
    for expected in [
        "n_123___module",
        "data_in",
        "data_in__2",
        "data_in__3",
        "data_in__4",
        "output_id",
        "output_id__2",
    ] {
        assert!(output.contains(expected), "missing identifier {expected}");
    }
}

#[test]
fn scalar_and_vector_declarations_and_constants_are_exact() {
    let scalar = compile(MINIMAL_AND, CompileOptions::default())
        .verilog
        .expect("scalar fixture compiles");
    assert!(scalar.contains("input wire a;"));
    assert!(!scalar.contains("[0:0]"));

    let vector = compile(EIGHT_BIT, CompileOptions::default())
        .verilog
        .expect("vector fixture compiles");
    assert!(vector.contains("input wire [7:0] data_in;"));
    assert!(vector.contains("wire [7:0] mask_value;"));
    assert!(vector.contains("8'b10100101"));

    let constant = compile(CONST_EXAMPLE, CompileOptions::default())
        .verilog
        .expect("CONST fixture compiles");
    assert!(constant.contains("4'b0011"));
}

#[test]
fn source_map_has_exact_one_based_inclusive_ranges_and_origins() {
    let result = compile(MINIMAL_AND, CompileOptions::default());
    let output = result.verilog.expect("fixture compiles");
    let map = result.source_map.expect("default emits a source map");
    let lines: Vec<_> = output.lines().collect();
    let entries = map.entries();
    assert_eq!(entries.len(), 11);

    assert_eq!(entries[0].kind, SourceMapKind::ModuleDeclaration);
    assert_eq!((entries[0].start_line, entries[0].end_line), (1, 5));
    assert_eq!(entries[0].source.circuit_id.as_deref(), Some("minimal-and"));
    assert_eq!(entries[0].source.field.as_deref(), Some("name"));

    let expected = [
        (
            SourceMapKind::PortDeclaration,
            7,
            None,
            None,
            Some("input-a"),
        ),
        (
            SourceMapKind::PortDeclaration,
            8,
            None,
            None,
            Some("input-b"),
        ),
        (
            SourceMapKind::PortDeclaration,
            9,
            None,
            None,
            Some("output-y"),
        ),
        (SourceMapKind::NetDeclaration, 11, None, Some("net-a"), None),
        (SourceMapKind::NetDeclaration, 12, None, Some("net-b"), None),
        (SourceMapKind::NetDeclaration, 13, None, Some("net-y"), None),
        (
            SourceMapKind::BoundaryAssignment,
            15,
            None,
            Some("net-a"),
            Some("input-a"),
        ),
        (
            SourceMapKind::BoundaryAssignment,
            16,
            None,
            Some("net-b"),
            Some("input-b"),
        ),
        (
            SourceMapKind::BoundaryAssignment,
            17,
            None,
            Some("net-y"),
            Some("output-y"),
        ),
        (
            SourceMapKind::ComponentAssignment,
            19,
            Some("and-1"),
            Some("net-y"),
            None,
        ),
    ];
    for (entry, (kind, line, component, net, port)) in entries[1..].iter().zip(expected) {
        assert_eq!(entry.kind, kind);
        assert_eq!((entry.start_line, entry.end_line), (line, line));
        assert_eq!(entry.source.component_id.as_deref(), component);
        assert_eq!(entry.source.net_id.as_deref(), net);
        assert_eq!(entry.source.port_id.as_deref(), port);
        assert!(line > 0 && line as usize <= lines.len());
        assert!(!lines[line as usize - 1].is_empty());
        assert!(entry.generated_identifier.is_safe());
    }
}

#[test]
fn source_map_can_be_disabled_without_changing_verilog() {
    let with_map = compile(MINIMAL_AND, CompileOptions::default());
    let without_map = compile(
        MINIMAL_AND,
        CompileOptions {
            emit_source_map: false,
        },
    );
    assert_eq!(without_map.verilog, with_map.verilog);
    assert_eq!(without_map.diagnostics, with_map.diagnostics);
    assert!(without_map.source_map.is_none());
}

#[test]
fn compile_result_is_repeatable_and_stably_serializable() {
    let first = compile(FULL_ADDER, CompileOptions::default());
    let second = compile(FULL_ADDER, CompileOptions::default());
    assert_eq!(first, second);
    let encoded = serde_json::to_string(&first).expect("compile result serializes");
    let decoded: jsonrtl::CompileResult =
        serde_json::from_str(&encoded).expect("compile result deserializes");
    assert_eq!(decoded, first);

    let source_map = first.source_map.expect("default emits a map");
    let identifiers: BTreeSet<_> = source_map
        .entries()
        .iter()
        .map(|entry| entry.generated_identifier.as_str())
        .collect();
    assert!(!identifiers.is_empty());
}

#[test]
fn hostile_typed_documents_do_not_panic_or_emit_output() {
    let mut document = CircuitDocument::from_json(MINIMAL_AND).expect("fixture parses");
    document.circuit.components[0].connections.clear();
    document.circuit.nets[0].width = u32::MAX;
    document.circuit.name = "\0\nmodule".repeat(32);
    let outcome = panic::catch_unwind(|| {
        Kernel::default().compile_verilog(&document, &CompileOptions::default())
    });
    let result = outcome.expect("compiler must not panic on typed hostile input");
    assert!(result.diagnostics.has_errors());
    assert!(result.verilog.is_none());
    assert!(result.source_map.is_none());
}

fn compile(input: &str, options: CompileOptions) -> jsonrtl::CompileResult {
    let document = CircuitDocument::from_json(input).expect("fixture parses");
    Kernel::default().compile_verilog(&document, &options)
}
