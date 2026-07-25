use std::{collections::BTreeSet, panic};

use jsonrtl::{
    CIRCUIT_V1_SCHEMA, COMPONENT_DEFINITIONS, CircuitDocument, ComponentType, KernelLimits,
    ParseError, PortDirection, component_definition,
};
use serde_json::{Value, json};

const MINIMAL_AND: &str = include_str!("../../../tests/fixtures/valid/minimal-and.json");
const HALF_ADDER: &str = include_str!("../../../tests/fixtures/valid/half-adder.json");
const FULL_ADDER: &str = include_str!("../../../tests/fixtures/valid/full-adder.json");
const EIGHT_BIT: &str = include_str!("../../../tests/fixtures/valid/eight-bit.json");
const CONST_EXAMPLE: &str = include_str!("../../../tests/fixtures/valid/const.json");

const MALFORMED: &str = include_str!("../../../tests/fixtures/invalid/malformed.json");
const MISSING_REQUIRED: &str =
    include_str!("../../../tests/fixtures/invalid/missing-required-field.json");
const INVALID_ENUM: &str = include_str!("../../../tests/fixtures/invalid/invalid-enum.json");
const WIDTH_ZERO: &str = include_str!("../../../tests/fixtures/invalid/width-zero.json");
const UNKNOWN_FIELD: &str =
    include_str!("../../../tests/fixtures/invalid/unknown-logical-field.json");
const UNSUPPORTED_VERSION: &str =
    include_str!("../../../tests/fixtures/invalid/unsupported-version.json");
const OVER_LIMIT_COMPONENTS: &str =
    include_str!("../../../tests/fixtures/invalid/over-limit-components.json");
const MULTIPLE_SCHEMA_ERRORS: &str =
    include_str!("../../../tests/fixtures/invalid/multiple-schema-errors.json");

#[test]
fn canonical_schema_is_valid_draft_2020_12() {
    let schema: Value = serde_json::from_str(CIRCUIT_V1_SCHEMA).expect("schema is JSON");
    assert!(jsonschema::meta::is_valid(&schema));
}

#[test]
fn every_valid_fixture_parses() {
    for (name, input) in [
        ("minimal AND", MINIMAL_AND),
        ("half adder", HALF_ADDER),
        ("full adder", FULL_ADDER),
        ("8-bit circuit", EIGHT_BIT),
        ("CONST", CONST_EXAMPLE),
    ] {
        CircuitDocument::from_json(input).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

#[test]
fn phase_zero_examples_parse_under_the_implemented_schema() {
    for input in [
        include_str!("../../../schemas/examples/minimal-and.json"),
        include_str!("../../../schemas/examples/half-adder.json"),
        include_str!("../../../schemas/examples/multi-bit.json"),
        include_str!("../../../schemas/examples/sliced-bus.json"),
    ] {
        CircuitDocument::from_json(input).expect("Phase 0 example must stay compatible");
    }
}

#[test]
fn editor_metadata_is_opaque_and_does_not_change_logical_circuit() {
    let mut first: Value = serde_json::from_str(MINIMAL_AND).expect("fixture is JSON");
    first["editorMetadata"] = json!({
        "unknown": { "deep": [1, true, null, { "color": "violet" }] },
        "zoom": 4.5
    });

    let mut second = first.clone();
    second["editorMetadata"] = json!(["any", "JSON", 42]);

    let first = CircuitDocument::from_json(&first.to_string()).expect("metadata is opaque");
    let second = CircuitDocument::from_json(&second.to_string()).expect("metadata is opaque");

    assert_eq!(first.circuit, second.circuit);
    assert_ne!(first.editor_metadata, second.editor_metadata);
}

#[test]
fn malformed_json_has_a_structured_error() {
    let error = CircuitDocument::from_json(MALFORMED).expect_err("fixture is malformed");
    let ParseError::MalformedJson { message, line, .. } = error else {
        panic!("expected malformed-JSON error, got {error:?}");
    };
    assert!(line > 0);
    assert!(!message.is_empty());
}

#[test]
fn missing_required_field_has_a_stable_reason() {
    assert_schema_error(MISSING_REQUIRED, "SCHEMA_REQUIRED_FIELD", "/circuit");
}

#[test]
fn invalid_enum_has_a_stable_reason() {
    assert_schema_error(
        INVALID_ENUM,
        "SCHEMA_INVALID_VALUE",
        "/circuit/components/0/type",
    );
}

#[test]
fn zero_width_has_a_stable_reason() {
    assert_schema_error(WIDTH_ZERO, "SCHEMA_MINIMUM", "/circuit/nets/0/width");
}

#[test]
fn unknown_logical_field_is_rejected() {
    assert_schema_error(UNKNOWN_FIELD, "SCHEMA_UNKNOWN_FIELD", "/circuit");
}

#[test]
fn unsupported_version_is_not_silently_reinterpreted() {
    let error =
        CircuitDocument::from_json(UNSUPPORTED_VERSION).expect_err("version is unsupported");
    assert_eq!(
        error,
        ParseError::UnsupportedSchemaVersion {
            found: "2.0".to_owned(),
            supported: jsonrtl::SUPPORTED_SCHEMA_VERSIONS,
        }
    );
}

#[test]
fn custom_limits_reject_an_oversized_document() {
    let limits = KernelLimits {
        max_components: 0,
        ..KernelLimits::default()
    };
    let error = CircuitDocument::from_json_with_limits(OVER_LIMIT_COMPONENTS, &limits)
        .expect_err("one component exceeds a zero-component test limit");

    let ParseError::ResourceLimits { diagnostics } = error else {
        panic!("expected resource-limit diagnostics, got {error:?}");
    };
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "LIMIT_COMPONENTS");
    assert_eq!(diagnostics[0].json_path, "/circuit/components");
    assert_eq!(diagnostics[0].actual, 1);
    assert_eq!(diagnostics[0].maximum, 0);
}

#[test]
fn every_configurable_logical_limit_is_enforced() {
    let cases = [
        (
            MINIMAL_AND,
            KernelLimits {
                max_ports: 2,
                ..KernelLimits::default()
            },
            "LIMIT_PORTS",
        ),
        (
            MINIMAL_AND,
            KernelLimits {
                max_nets: 2,
                ..KernelLimits::default()
            },
            "LIMIT_NETS",
        ),
        (
            MINIMAL_AND,
            KernelLimits {
                max_width: 0,
                ..KernelLimits::default()
            },
            "LIMIT_WIDTH",
        ),
        (
            MINIMAL_AND,
            KernelLimits {
                max_string_length: 2,
                ..KernelLimits::default()
            },
            "LIMIT_STRING_LENGTH",
        ),
        (
            CONST_EXAMPLE,
            KernelLimits {
                max_parameters_per_component: 0,
                ..KernelLimits::default()
            },
            "LIMIT_PARAMETERS",
        ),
    ];

    for (input, limits, expected_code) in cases {
        let error = CircuitDocument::from_json_with_limits(input, &limits)
            .expect_err("stricter test limit must be enforced");
        let ParseError::ResourceLimits { diagnostics } = error else {
            panic!("expected resource-limit diagnostics, got {error:?}");
        };
        assert!(
            diagnostics.iter().any(|item| item.code == expected_code),
            "missing {expected_code}: {diagnostics:#?}"
        );
    }
}

#[test]
fn byte_limit_is_checked_before_json_parsing() {
    let limits = KernelLimits {
        max_document_bytes: 8,
        ..KernelLimits::default()
    };
    assert_eq!(
        CircuitDocument::from_json_with_limits(MALFORMED, &limits),
        Err(ParseError::DocumentTooLarge {
            actual: MALFORMED.len(),
            maximum: 8,
        })
    );
}

#[test]
fn schema_error_order_is_stable() {
    let first = CircuitDocument::from_json(MULTIPLE_SCHEMA_ERRORS).expect_err("invalid fixture");
    let second = CircuitDocument::from_json(MULTIPLE_SCHEMA_ERRORS).expect_err("invalid fixture");
    assert_eq!(first, second);

    let ParseError::Schema { diagnostics } = first else {
        panic!("expected schema diagnostics");
    };
    assert!(diagnostics.len() >= 3);
    assert!(diagnostics.windows(2).all(|pair| {
        (
            &pair[0].json_path,
            &pair[0].code,
            &pair[0].schema_path,
            &pair[0].message,
        ) <= (
            &pair[1].json_path,
            &pair[1].code,
            &pair[1].schema_path,
            &pair[1].message,
        )
    }));
}

#[test]
fn malformed_and_hostile_inputs_never_panic() {
    let deeply_nested = format!("{}null{}", "[".repeat(256), "]".repeat(256));
    let oversized_string = format!(
        r#"{{"schemaVersion":"1.0","padding":"{}"}}"#,
        "x".repeat(20_000)
    );

    for input in [
        "",
        "{",
        "[]",
        "null",
        MALFORMED,
        &deeply_nested,
        &oversized_string,
    ] {
        let result = panic::catch_unwind(|| CircuitDocument::from_json(input));
        assert!(result.is_ok(), "parser panicked for hostile input");
        assert!(result.expect("checked above").is_err());
    }
}

#[test]
fn serialization_round_trips_logical_content() {
    for input in [
        MINIMAL_AND,
        HALF_ADDER,
        FULL_ADDER,
        EIGHT_BIT,
        CONST_EXAMPLE,
    ] {
        let first = CircuitDocument::from_json(input).expect("valid fixture");
        let serialized = serde_json::to_string(&first).expect("public model serializes");
        let second = CircuitDocument::from_json(&serialized).expect("serialized model parses");
        assert_eq!(first, second);
    }
}

#[test]
fn component_catalog_has_unique_ports_and_one_output() {
    assert_eq!(COMPONENT_DEFINITIONS.len(), 9);

    let mut types = BTreeSet::new();
    for definition in COMPONENT_DEFINITIONS {
        assert!(types.insert(definition.component_type));
        assert_eq!(
            component_definition(definition.component_type),
            Some(definition)
        );

        let names: BTreeSet<_> = definition.ports.iter().map(|port| port.name).collect();
        assert_eq!(names.len(), definition.ports.len());
        assert_eq!(
            definition
                .ports
                .iter()
                .filter(|port| port.direction == PortDirection::Output)
                .count(),
            1
        );
        assert_eq!(
            definition
                .ports
                .iter()
                .filter(|port| port.direction == PortDirection::Input)
                .count(),
            definition.input_arity
        );
    }

    assert_eq!(
        component_definition(ComponentType::Const)
            .expect("CONST is catalogued")
            .required_parameters,
        &["value"]
    );
}

#[test]
fn crate_manifests_enforce_dependency_direction() {
    const CORE_MANIFEST: &str = include_str!("../Cargo.toml");
    const CLI_MANIFEST: &str = include_str!("../../jsonrtl-cli/Cargo.toml");
    const API_MANIFEST: &str = include_str!("../../jsonrtl-api/Cargo.toml");

    for forbidden in ["axum", "tokio", "clap", "yosy", "librelane"] {
        assert!(
            !CORE_MANIFEST.to_ascii_lowercase().contains(forbidden),
            "core manifest must not depend on {forbidden}"
        );
    }
    assert!(CLI_MANIFEST.contains("jsonrtl = { path = \"../jsonrtl\" }"));
    assert!(API_MANIFEST.contains("jsonrtl = { path = \"../jsonrtl\" }"));
}

fn assert_schema_error(input: &str, expected_code: &str, expected_path: &str) {
    let error = CircuitDocument::from_json(input).expect_err("fixture must be rejected");
    let ParseError::Schema { diagnostics } = error else {
        panic!("expected schema error, got {error:?}");
    };
    assert!(
        diagnostics
            .iter()
            .any(|item| item.code == expected_code && item.json_path == expected_path),
        "missing {expected_code} at {expected_path}: {diagnostics:#?}"
    );
}
