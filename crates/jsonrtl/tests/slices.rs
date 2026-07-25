//! Canonical schema v1.1: connections that reference a bit slice of a net.
//!
//! Slices exist so a bus can be split and re-merged, which means the
//! single-driver rule has to hold per bit rather than per net. These tests pin
//! that boundary and the Verilog it produces.

use jsonrtl::{CircuitDocument, CompileOptions, DiagnosticCode, Kernel, ParseError};
use serde_json::{Value, json};

/// A one-net circuit: `wide` is `width` bits, driven and read as described by
/// `components`, with one input port `a` and one output port `y`.
fn document(version: &str, wide_width: u32, components: Value) -> CircuitDocument {
    let value = json!({
        "schemaVersion": version,
        "circuit": {
            "id": "c", "name": "sliced",
            "ports": [
                { "id": "p_in", "name": "a", "direction": "input", "width": wide_width, "netId": "src" },
                { "id": "p_out", "name": "y", "direction": "output", "width": wide_width, "netId": "wide" }
            ],
            "components": components,
            "nets": [
                { "id": "src", "name": "src", "width": wide_width },
                { "id": "wide", "name": "wide", "width": wide_width }
            ]
        }
    });
    CircuitDocument::from_json(&value.to_string()).expect("fixture parses")
}

/// One NOT per bit: bit `k` of `wide` is driven from bit `k` of `src`.
fn per_bit_inverters(width: u32) -> Value {
    Value::Array(
        (0..width)
            .map(|bit| {
                json!({
                    "id": format!("inv{bit}"), "name": format!("inv{bit}"),
                    "type": "NOT", "width": 1,
                    "connections": {
                        "A": { "net": "src", "msb": bit, "lsb": bit },
                        "Y": { "net": "wide", "msb": bit, "lsb": bit }
                    },
                    "parameters": {}
                })
            })
            .collect(),
    )
}

fn codes(document: &CircuitDocument) -> Vec<DiagnosticCode> {
    Kernel::default()
        .validate(document)
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn a_v1_0_document_still_parses_with_plain_string_connections() {
    let input = include_str!("../../../tests/fixtures/valid/minimal-and.json");
    let document = CircuitDocument::from_json(input).expect("v1.0 fixture parses");
    assert_eq!(document.schema_version.as_str(), "1.0");
    assert!(!Kernel::default().validate(&document).has_errors());
}

#[test]
fn every_bit_of_a_net_may_have_its_own_driver() {
    // The point of slices: eight separate drivers on one 8-bit net is exactly
    // what a bus merger produces and must not be a multiple-driver error.
    let document = document("1.1", 8, per_bit_inverters(8));
    let report = Kernel::default().validate(&document);
    assert!(!report.has_errors(), "{:?}", report.diagnostics());
}

#[test]
fn two_drivers_on_one_bit_are_still_an_error() {
    let mut components = per_bit_inverters(4);
    components.as_array_mut().unwrap().push(json!({
        "id": "clash", "name": "clash", "type": "BUFFER", "width": 1,
        "connections": {
            "A": { "net": "src", "msb": 0, "lsb": 0 },
            "Y": { "net": "wide", "msb": 0, "lsb": 0 }
        },
        "parameters": {}
    }));
    let document = document("1.1", 4, components);
    assert!(codes(&document).contains(&DiagnosticCode::NetMultipleDrivers));
    let message = Kernel::default()
        .validate(&document)
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code == DiagnosticCode::NetMultipleDrivers)
        .expect("multiple drivers")
        .message
        .clone();
    assert!(message.contains("Bit 0"), "{message}");
}

#[test]
fn overlapping_wide_slices_are_a_multiple_driver_error() {
    let components = json!([
        { "id": "low", "name": "low", "type": "BUFFER", "width": 4,
          "connections": { "A": { "net": "src", "msb": 3, "lsb": 0 },
                           "Y": { "net": "wide", "msb": 3, "lsb": 0 } },
          "parameters": {} },
        { "id": "mid", "name": "mid", "type": "BUFFER", "width": 4,
          "connections": { "A": { "net": "src", "msb": 5, "lsb": 2 },
                           "Y": { "net": "wide", "msb": 5, "lsb": 2 } },
          "parameters": {} }
    ]);
    let document = document("1.1", 8, components);
    assert!(codes(&document).contains(&DiagnosticCode::NetMultipleDrivers));
}

#[test]
fn a_consumed_bit_with_no_driver_is_an_error_even_when_other_bits_are_driven() {
    // Only bits 0..3 are driven, but the output port reads all eight.
    let document = document("1.1", 8, per_bit_inverters(4));
    assert!(codes(&document).contains(&DiagnosticCode::NetNoDriver));
}

#[test]
fn a_slice_beyond_the_end_of_its_net_is_rejected() {
    let components = json!([
        { "id": "over", "name": "over", "type": "BUFFER", "width": 1,
          "connections": { "A": { "net": "src", "msb": 0, "lsb": 0 },
                           "Y": { "net": "wide", "msb": 9, "lsb": 9 } },
          "parameters": {} }
    ]);
    assert!(codes(&document("1.1", 4, components)).contains(&DiagnosticCode::SliceOutOfRange));
}

#[test]
fn an_inverted_slice_range_is_rejected() {
    let components = json!([
        { "id": "back", "name": "back", "type": "BUFFER", "width": 2,
          "connections": { "A": { "net": "src", "msb": 0, "lsb": 3 },
                           "Y": { "net": "wide", "msb": 1, "lsb": 0 } },
          "parameters": {} }
    ]);
    assert!(codes(&document("1.1", 4, components)).contains(&DiagnosticCode::SliceOutOfRange));
}

#[test]
fn a_slice_whose_width_differs_from_the_component_is_rejected() {
    let components = json!([
        { "id": "narrow", "name": "narrow", "type": "BUFFER", "width": 4,
          "connections": { "A": { "net": "src", "msb": 1, "lsb": 0 },
                           "Y": { "net": "wide", "msb": 3, "lsb": 0 } },
          "parameters": {} }
    ]);
    assert!(
        codes(&document("1.1", 4, components)).contains(&DiagnosticCode::WidthComponentNetMismatch)
    );
}

#[test]
fn slices_are_rejected_in_a_document_declaring_schema_1_0() {
    let document = document("1.0", 2, per_bit_inverters(2));
    assert!(codes(&document).contains(&DiagnosticCode::SliceRequiresSchema11));
}

#[test]
fn an_unknown_schema_version_is_still_refused() {
    let error = CircuitDocument::from_json(
        &json!({ "schemaVersion": "2.0", "circuit": {
            "id": "c", "name": "c", "ports": [], "components": [], "nets": [] } })
        .to_string(),
    )
    .unwrap_err();
    assert!(matches!(error, ParseError::UnsupportedSchemaVersion { .. }));
}

#[test]
fn routing_different_bits_through_a_net_is_not_a_combinational_cycle() {
    // `rot` reads bit 0 of `wide` and drives bit 1; `feed` drives bit 0 from
    // the input. Net-level cycle detection would wrongly flag this.
    let components = json!([
        { "id": "feed", "name": "feed", "type": "BUFFER", "width": 1,
          "connections": { "A": { "net": "src", "msb": 0, "lsb": 0 },
                           "Y": { "net": "wide", "msb": 0, "lsb": 0 } },
          "parameters": {} },
        { "id": "rot", "name": "rot", "type": "NOT", "width": 1,
          "connections": { "A": { "net": "wide", "msb": 0, "lsb": 0 },
                           "Y": { "net": "wide", "msb": 1, "lsb": 1 } },
          "parameters": {} }
    ]);
    let document = document("1.1", 2, components);
    let report = Kernel::default().validate(&document);
    assert!(
        !report
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::GraphCombinationalCycle),
        "{:?}",
        report.diagnostics()
    );
}

#[test]
fn a_genuine_cycle_through_one_bit_is_still_detected() {
    let components = json!([
        { "id": "loop_a", "name": "loop_a", "type": "NOT", "width": 1,
          "connections": { "A": { "net": "wide", "msb": 1, "lsb": 1 },
                           "Y": { "net": "wide", "msb": 0, "lsb": 0 } },
          "parameters": {} },
        { "id": "loop_b", "name": "loop_b", "type": "NOT", "width": 1,
          "connections": { "A": { "net": "wide", "msb": 0, "lsb": 0 },
                           "Y": { "net": "wide", "msb": 1, "lsb": 1 } },
          "parameters": {} }
    ]);
    let document = document("1.1", 2, components);
    assert!(codes(&document).contains(&DiagnosticCode::GraphCombinationalCycle));
}

#[test]
fn sliced_connections_emit_indexed_verilog() {
    let document = document("1.1", 2, per_bit_inverters(2));
    let result = Kernel::default().compile_verilog(&document, &CompileOptions::default());
    let verilog = result.verilog.expect("compiles");
    assert!(verilog.contains("assign wide[0] = ~src[0];"), "{verilog}");
    assert!(verilog.contains("assign wide[1] = ~src[1];"), "{verilog}");
    assert!(verilog.contains("wire [1:0] wide;"), "{verilog}");
}

#[test]
fn a_multi_bit_slice_emits_a_range() {
    let components = json!([
        { "id": "half", "name": "half", "type": "BUFFER", "width": 4,
          "connections": { "A": { "net": "src", "msb": 3, "lsb": 0 },
                           "Y": { "net": "wide", "msb": 7, "lsb": 4 } },
          "parameters": {} },
        { "id": "rest", "name": "rest", "type": "BUFFER", "width": 4,
          "connections": { "A": { "net": "src", "msb": 7, "lsb": 4 },
                           "Y": { "net": "wide", "msb": 3, "lsb": 0 } },
          "parameters": {} }
    ]);
    let document = document("1.1", 8, components);
    let verilog = Kernel::default()
        .compile_verilog(&document, &CompileOptions::default())
        .verilog
        .expect("compiles");
    assert!(
        verilog.contains("assign wide[7:4] = src[3:0];"),
        "{verilog}"
    );
    assert!(
        verilog.contains("assign wide[3:0] = src[7:4];"),
        "{verilog}"
    );
}

#[test]
fn a_wide_net_does_not_allocate_per_bit() {
    // Validation must stay interval-based: a 4096-bit net (the kernel maximum)
    // with a handful of endpoints must not build per-bit state.
    let document = document("1.1", 4096, per_bit_inverters(2));
    let report = Kernel::default().validate(&document);
    // Bits 2..4095 are read by the output port but undriven, so this reports
    // exactly one NET_NO_DRIVER rather than one per bit.
    assert_eq!(
        report
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::NetNoDriver)
            .count(),
        1
    );
}
