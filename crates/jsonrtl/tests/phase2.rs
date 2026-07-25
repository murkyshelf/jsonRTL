use std::{
    collections::{BTreeMap, BTreeSet},
    panic,
};

use jsonrtl::{
    Circuit, CircuitDocument, Component, ComponentType, DIAGNOSTIC_CODES, Diagnostic,
    DiagnosticCode, DiagnosticSeverity, Kernel, KernelLimits, ModulePort, Net, Parameters,
    PortDirection, SchemaVersion, ValidationReport,
};
use serde_json::json;

const MINIMAL_AND: &str = include_str!("../../../tests/fixtures/valid/minimal-and.json");
const CONST_EXAMPLE: &str = include_str!("../../../tests/fixtures/valid/const.json");
const COMBINED_INVALID: &str =
    include_str!("../../../tests/fixtures/semantic/combined-invalid.json");
const SELF_LOOP: &str = include_str!("../../../tests/fixtures/semantic/self-loop.json");

#[test]
fn diagnostic_registry_is_complete_unique_and_documented() {
    const DOCUMENTATION: &str = include_str!("../../../docs/diagnostics.md");
    assert_eq!(DIAGNOSTIC_CODES.len(), 34);
    let mut spellings = BTreeSet::new();
    for code in DIAGNOSTIC_CODES {
        assert!(spellings.insert(code.as_str()), "duplicate code {code}");
        assert_eq!(
            serde_json::to_string(code).expect("code serializes"),
            format!("\"{}\"", code.as_str()),
            "serialized spelling drifted for {code}"
        );
        assert!(
            DOCUMENTATION.contains(&format!("`{}`", code.as_str())),
            "undocumented diagnostic code {code}"
        );
    }
}

#[test]
fn valid_document_has_no_blocking_diagnostics() {
    let report = validate(&minimal_and());
    assert!(!report.has_errors());
    assert_eq!(report.errors().count(), 0);
    assert!(report.warnings().count() >= 1);
}

#[test]
fn duplicate_component_ids_are_errors() {
    let mut document = minimal_and();
    document
        .circuit
        .components
        .push(document.circuit.components[0].clone());
    assert_error_source(
        &validate(&document),
        DiagnosticCode::IdDuplicateComponent,
        Some("and-1"),
        None,
        None,
        "id",
    );
}

#[test]
fn duplicate_net_ids_are_errors() {
    let mut document = minimal_and();
    document.circuit.nets.push(document.circuit.nets[0].clone());
    assert_error_source(
        &validate(&document),
        DiagnosticCode::IdDuplicateNet,
        None,
        Some("net-a"),
        None,
        "id",
    );
}

#[test]
fn duplicate_module_port_ids_are_errors() {
    let mut document = minimal_and();
    document.circuit.ports[1].id = document.circuit.ports[0].id.clone();
    assert_error_source(
        &validate(&document),
        DiagnosticCode::IdDuplicatePort,
        None,
        None,
        Some("input-a"),
        "id",
    );
}

#[test]
fn duplicate_external_port_names_are_errors() {
    let mut document = minimal_and();
    document.circuit.ports[1].name = document.circuit.ports[0].name.clone();
    assert_code(&validate(&document), DiagnosticCode::NameDuplicatePort);
}

#[test]
fn empty_circuit_name_is_an_error() {
    let mut document = minimal_and();
    document.circuit.name = "   ".into();
    assert_error_source(
        &validate(&document),
        DiagnosticCode::NameEmpty,
        None,
        None,
        None,
        "name",
    );
}

#[test]
fn invalid_circuit_name_is_an_error() {
    let mut document = minimal_and();
    document.circuit.name = "bad\tname".into();
    assert_error_source(
        &validate(&document),
        DiagnosticCode::NameInvalid,
        None,
        None,
        None,
        "name",
    );
}

#[test]
fn empty_port_name_is_an_error() {
    let mut document = minimal_and();
    document.circuit.ports[0].name.clear();
    assert_error_source(
        &validate(&document),
        DiagnosticCode::NameEmpty,
        None,
        None,
        Some("input-a"),
        "name",
    );
}

#[test]
fn invalid_port_name_is_an_error() {
    let mut document = minimal_and();
    document.circuit.ports[0].name = "bad\nname".into();
    assert_error_source(
        &validate(&document),
        DiagnosticCode::NameInvalid,
        None,
        None,
        Some("input-a"),
        "name",
    );
}

#[test]
fn empty_component_name_is_an_error() {
    let mut document = minimal_and();
    document.circuit.components[0].name.clear();
    assert_error_source(
        &validate(&document),
        DiagnosticCode::NameEmpty,
        Some("and-1"),
        None,
        None,
        "name",
    );
}

#[test]
fn invalid_component_name_is_an_error() {
    let mut document = minimal_and();
    document.circuit.components[0].name = "\u{0}".into();
    assert_error_source(
        &validate(&document),
        DiagnosticCode::NameInvalid,
        Some("and-1"),
        None,
        None,
        "name",
    );
}

#[test]
fn configured_string_length_is_enforced_semantically() {
    let document = minimal_and();
    let kernel = Kernel::new(KernelLimits {
        max_string_length: 3,
        ..KernelLimits::default()
    });
    assert_code(
        &kernel.validate(&document),
        DiagnosticCode::LimitStringLength,
    );
}

#[test]
fn verilog_keywords_and_required_sanitization_are_warnings() {
    let mut document = minimal_and();
    document.circuit.name = "module".into();
    document.circuit.ports[0].name = "data in".into();
    let report = validate(&document);
    assert_code(&report, DiagnosticCode::NameVerilogKeyword);
    assert_code(&report, DiagnosticCode::NameRequiresSanitization);
    assert!(!report.has_errors());
}

#[test]
fn post_sanitization_collisions_are_reported() {
    let mut document = minimal_and();
    document.circuit.ports[0].name = "data-in".into();
    document.circuit.ports[1].name = "data in".into();
    let report = validate(&document);
    let diagnostic = first(&report, DiagnosticCode::NameSanitizationCollision);
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
    assert_eq!(diagnostic.related_sources.len(), 1);
}

#[test]
fn module_port_unknown_net_reference_is_an_error() {
    let mut document = minimal_and();
    document.circuit.ports[0].net_id = "missing".into();
    assert_error_source(
        &validate(&document),
        DiagnosticCode::NetUnknownReference,
        None,
        Some("missing"),
        Some("input-a"),
        "netId",
    );
}

#[test]
fn component_unknown_net_reference_is_an_error() {
    let mut document = minimal_and();
    document.circuit.components[0]
        .connections
        .insert("A".into(), "missing".into());
    assert_error_source(
        &validate(&document),
        DiagnosticCode::NetUnknownReference,
        Some("and-1"),
        Some("missing"),
        None,
        "connections.A",
    );
}

#[test]
fn missing_required_logical_connection_is_an_error() {
    let mut document = minimal_and();
    document.circuit.components[0].connections.remove("B");
    assert_error_source(
        &validate(&document),
        DiagnosticCode::ComponentMissingConnection,
        Some("and-1"),
        None,
        None,
        "connections.B",
    );
}

#[test]
fn unknown_logical_connection_is_an_error() {
    let mut document = minimal_and();
    document.circuit.components[0]
        .connections
        .insert("Q".into(), "net-a".into());
    assert_error_source(
        &validate(&document),
        DiagnosticCode::ComponentUnknownConnection,
        Some("and-1"),
        None,
        None,
        "connections.Q",
    );
}

#[test]
fn component_type_outside_catalog_is_an_error() {
    let mut document = minimal_and();
    document.circuit.components[0].component_type = ComponentType::Unknown;
    assert_error_source(
        &validate(&document),
        DiagnosticCode::ComponentUnknownType,
        Some("and-1"),
        None,
        None,
        "type",
    );
}

#[test]
fn missing_required_parameter_is_an_error() {
    let mut document = const_document();
    document.circuit.components[0].parameters.remove("value");
    assert_error_source(
        &validate(&document),
        DiagnosticCode::ComponentMissingParameter,
        Some("constant-1"),
        None,
        None,
        "parameters.value",
    );
}

#[test]
fn unknown_parameter_is_an_error() {
    let mut document = minimal_and();
    document.circuit.components[0]
        .parameters
        .insert("delay".into(), json!(1));
    assert_error_source(
        &validate(&document),
        DiagnosticCode::ComponentUnknownParameter,
        Some("and-1"),
        None,
        None,
        "parameters.delay",
    );
}

#[test]
fn zero_width_is_defended_at_typed_boundary() {
    let mut document = minimal_and();
    document.circuit.ports[0].width = 0;
    document.circuit.components[0].width = 0;
    document.circuit.nets[0].width = 0;
    let report = validate(&document);
    assert_eq!(count(&report, DiagnosticCode::WidthZero), 3);
}

#[test]
fn excessive_width_is_defended_at_typed_boundary() {
    let mut document = minimal_and();
    document.circuit.nets[0].width = KernelLimits::default().max_width + 1;
    assert_error_source(
        &validate(&document),
        DiagnosticCode::WidthExceedsLimit,
        None,
        Some("net-a"),
        None,
        "width",
    );
}

#[test]
fn external_port_and_net_width_mismatch_is_an_error() {
    let mut document = minimal_and();
    document.circuit.ports[0].width = 2;
    assert_error_source(
        &validate(&document),
        DiagnosticCode::WidthPortNetMismatch,
        None,
        Some("net-a"),
        Some("input-a"),
        "width",
    );
}

#[test]
fn gate_connection_width_mismatch_is_an_error() {
    let mut document = minimal_and();
    document.circuit.nets[0].width = 2;
    assert_error_source(
        &validate(&document),
        DiagnosticCode::WidthComponentNetMismatch,
        Some("and-1"),
        Some("net-a"),
        None,
        "connections.A",
    );
}

#[test]
fn malformed_const_literal_is_an_error() {
    let mut document = const_document();
    document.circuit.components[0]
        .parameters
        .insert("value".into(), json!("01x1"));
    assert_error_source(
        &validate(&document),
        DiagnosticCode::ConstLiteralMalformed,
        Some("constant-1"),
        None,
        None,
        "parameters.value",
    );
}

#[test]
fn const_literal_must_fit_exact_width() {
    let mut document = const_document();
    document.circuit.components[0]
        .parameters
        .insert("value".into(), json!("11"));
    assert_error_source(
        &validate(&document),
        DiagnosticCode::ConstValueWidthMismatch,
        Some("constant-1"),
        None,
        None,
        "parameters.value",
    );
}

#[test]
fn multiple_net_drivers_are_an_error() {
    let mut document = minimal_and();
    document.circuit.ports.push(ModulePort {
        id: "extra-driver".into(),
        name: "extra".into(),
        direction: PortDirection::Input,
        width: 1,
        net_id: "net-y".into(),
    });
    let diagnostic = first(&validate(&document), DiagnosticCode::NetMultipleDrivers).clone();
    assert_eq!(diagnostic.source.net_id.as_deref(), Some("net-y"));
    assert_eq!(diagnostic.related_sources.len(), 2);
}

#[test]
fn external_output_without_driver_is_an_error() {
    let mut document = minimal_and();
    document.circuit.components.clear();
    let report = validate(&document);
    let diagnostic = report
        .diagnostics()
        .iter()
        .find(|item| {
            item.code == DiagnosticCode::NetNoDriver
                && item.source.net_id.as_deref() == Some("net-y")
        })
        .expect("output net must have no-driver error");
    assert!(
        diagnostic
            .related_sources
            .iter()
            .any(|source| source.port_id.as_deref() == Some("output-y"))
    );
}

#[test]
fn connected_component_input_without_driver_is_an_error() {
    let mut document = minimal_and();
    document
        .circuit
        .ports
        .retain(|port| port.id.as_str() != "input-a");
    let diagnostic = validate(&document)
        .diagnostics()
        .iter()
        .find(|item| {
            item.code == DiagnosticCode::NetNoDriver
                && item.source.net_id.as_deref() == Some("net-a")
        })
        .expect("component input net must have no-driver error")
        .clone();
    assert!(diagnostic.related_sources.iter().any(|source| {
        source.component_id.as_deref() == Some("and-1")
            && source.field.as_deref() == Some("connections.A")
    }));
}

#[test]
fn driven_net_without_consumers_is_a_warning() {
    let mut document = minimal_and();
    document
        .circuit
        .ports
        .retain(|port| port.id.as_str() != "output-y");
    let diagnostic = first(&validate(&document), DiagnosticCode::NetNoConsumers).clone();
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
    assert_eq!(diagnostic.source.net_id.as_deref(), Some("net-y"));
    assert!(diagnostic.related_sources.iter().any(|source| {
        source.component_id.as_deref() == Some("and-1")
            && source.field.as_deref() == Some("connections.Y")
    }));
}

#[test]
fn fully_unused_declared_net_is_a_warning() {
    let mut document = minimal_and();
    document.circuit.nets.push(Net {
        id: "unused".into(),
        name: "unused".into(),
        width: 1,
    });
    let report = validate(&document);
    let diagnostic = first(&report, DiagnosticCode::NetUnused);
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
    assert_eq!(diagnostic.source.net_id.as_deref(), Some("unused"));
}

#[test]
fn self_loop_is_a_cycle_with_stable_sources() {
    let document = CircuitDocument::from_json(SELF_LOOP).expect("fixture parses");
    let report = validate(&document);
    let diagnostic = first(&report, DiagnosticCode::GraphCombinationalCycle);
    assert_eq!(diagnostic.source.component_id.as_deref(), Some("buffer-1"));
    assert!(
        diagnostic
            .related_sources
            .iter()
            .any(|source| source.net_id.as_deref() == Some("loop-net"))
    );
}

#[test]
fn two_node_cycle_is_detected() {
    let document = cycle_pairs(&[("a", "b")]);
    let report = validate(&document);
    assert_eq!(count(&report, DiagnosticCode::GraphCombinationalCycle), 1);
    let diagnostic = first(&report, DiagnosticCode::GraphCombinationalCycle);
    assert_eq!(diagnostic.source.component_id.as_deref(), Some("a"));
    assert!(
        diagnostic
            .related_sources
            .iter()
            .any(|source| source.component_id.as_deref() == Some("b"))
    );
    let related_nets: BTreeSet<_> = diagnostic
        .related_sources
        .iter()
        .filter_map(|source| source.net_id.as_deref())
        .collect();
    assert_eq!(related_nets, BTreeSet::from(["net-a-b", "net-b-a"]));
}

#[test]
fn multiple_disjoint_cycles_are_each_detected() {
    let document = cycle_pairs(&[("a", "b"), ("c", "d")]);
    assert_eq!(
        count(
            &validate(&document),
            DiagnosticCode::GraphCombinationalCycle
        ),
        2
    );
}

#[test]
fn large_acyclic_graph_uses_bounded_non_recursive_detection() {
    let document = large_acyclic(5_000);
    let result = panic::catch_unwind(|| validate(&document));
    let report = result.expect("validation must not recurse or panic");
    assert!(!report.has_errors(), "{:#?}", report.diagnostics());
    assert_eq!(count(&report, DiagnosticCode::GraphCombinationalCycle), 0);
}

#[test]
fn semantic_limits_stop_untrusted_typed_models_before_deep_work() {
    let document = large_acyclic(4);
    for (limits, code) in [
        (
            KernelLimits {
                max_ports: 1,
                ..KernelLimits::default()
            },
            DiagnosticCode::LimitPorts,
        ),
        (
            KernelLimits {
                max_components: 3,
                ..KernelLimits::default()
            },
            DiagnosticCode::LimitComponents,
        ),
        (
            KernelLimits {
                max_nets: 4,
                ..KernelLimits::default()
            },
            DiagnosticCode::LimitNets,
        ),
    ] {
        let report = Kernel::new(limits).validate(&document);
        assert_code(&report, code);
        assert!(report.has_errors());
    }

    let report = Kernel::new(KernelLimits {
        max_parameters_per_component: 0,
        ..KernelLimits::default()
    })
    .validate(&const_document());
    assert_code(&report, DiagnosticCode::LimitParameters);
}

#[test]
fn combined_invalid_document_collects_useful_independent_problems() {
    let document = CircuitDocument::from_json(COMBINED_INVALID).expect("fixture is schema-valid");
    let report = validate(&document);
    for code in [
        DiagnosticCode::IdDuplicatePort,
        DiagnosticCode::NameDuplicatePort,
        DiagnosticCode::NetUnknownReference,
        DiagnosticCode::ComponentMissingConnection,
        DiagnosticCode::ComponentUnknownConnection,
        DiagnosticCode::ComponentUnknownParameter,
        DiagnosticCode::WidthComponentNetMismatch,
        DiagnosticCode::ConstLiteralMalformed,
        DiagnosticCode::NetMultipleDrivers,
        DiagnosticCode::NetUnused,
    ] {
        assert_code(&report, code);
    }
    assert!(report.errors().count() >= 9);
    assert!(report.warnings().count() >= 1);
}

#[test]
fn permuting_arrays_does_not_change_ordered_diagnostics() {
    let document = CircuitDocument::from_json(COMBINED_INVALID).expect("fixture parses");
    let mut permuted = document.clone();
    permuted.circuit.ports.reverse();
    permuted.circuit.components.reverse();
    permuted.circuit.nets.reverse();

    let original_report = validate(&document);
    let permuted_report = validate(&permuted);
    assert_eq!(original_report, permuted_report);
    assert_eq!(
        serde_json::to_string(&original_report).expect("report serializes"),
        serde_json::to_string(&permuted_report).expect("report serializes")
    );
}

#[test]
fn validation_report_gates_errors_but_not_warnings_and_serializes_stably() {
    let warning_report = validate(&minimal_and());
    assert!(!warning_report.has_errors());
    assert_eq!(warning_report.errors().count(), 0);
    assert!(warning_report.warnings().count() > 0);

    let error_report =
        validate(&CircuitDocument::from_json(COMBINED_INVALID).expect("combined fixture parses"));
    assert!(error_report.has_errors());
    assert!(error_report.errors().count() > 0);

    let encoded = serde_json::to_string(&error_report).expect("report serializes");
    let decoded: ValidationReport = serde_json::from_str(&encoded).expect("report deserializes");
    assert_eq!(error_report, decoded);
    assert!(
        error_report
            .diagnostics()
            .windows(2)
            .all(|pair| pair[0].ordering_key <= pair[1].ordering_key)
    );
}

#[test]
fn hostile_typed_values_do_not_panic_or_allocate_from_width() {
    let mut document = minimal_and();
    document.circuit.components[0].width = u32::MAX;
    document.circuit.nets[0].width = u32::MAX;
    for index in 0..10_000 {
        document.circuit.components[0]
            .connections
            .insert(format!("unknown_{index}"), "net-a".into());
    }
    let result = panic::catch_unwind(|| validate(&document));
    let report = result.expect("semantic validator must not panic");
    assert!(report.has_errors());
    assert_code(&report, DiagnosticCode::WidthExceedsLimit);
    assert_code(&report, DiagnosticCode::ComponentUnknownConnection);
}

#[test]
fn generated_typed_models_are_panic_safe_and_permutation_invariant() {
    let mut state = 0x5eed_u64;
    for case in 0..128 {
        let component_count = (next_u64(&mut state) % 24 + 1) as usize;
        let mut document = large_acyclic(component_count);

        match next_u64(&mut state) % 7 {
            0 => document.circuit.components[0].width = 0,
            1 => document.circuit.components[0].width = u32::MAX,
            2 => {
                document.circuit.components[0].connections.remove("A");
            }
            3 => {
                document.circuit.components[0]
                    .connections
                    .insert("A".into(), format!("missing-{case}").into());
            }
            4 => document.circuit.nets.push(Net {
                id: format!("unused-{case}"),
                name: format!("unused_{case}"),
                width: 1,
            }),
            5 => document.circuit.ports[0].name = "data in".into(),
            _ => {
                document.circuit.components[0]
                    .parameters
                    .insert("unexpected".into(), json!(case));
            }
        }

        let original = panic::catch_unwind(|| validate(&document))
            .unwrap_or_else(|_| panic!("validator panicked for generated case {case}"));
        let mut permuted = document.clone();
        permuted.circuit.ports.reverse();
        permuted.circuit.components.reverse();
        permuted.circuit.nets.reverse();
        let permuted = panic::catch_unwind(|| validate(&permuted))
            .unwrap_or_else(|_| panic!("validator panicked for permutation {case}"));
        assert_eq!(original, permuted, "permutation changed case {case}");
    }
}

fn validate(document: &CircuitDocument) -> ValidationReport {
    Kernel::default().validate(document)
}

fn minimal_and() -> CircuitDocument {
    CircuitDocument::from_json(MINIMAL_AND).expect("minimal AND fixture parses")
}

fn const_document() -> CircuitDocument {
    CircuitDocument::from_json(CONST_EXAMPLE).expect("CONST fixture parses")
}

fn cycle_pairs(pairs: &[(&str, &str)]) -> CircuitDocument {
    let mut components = Vec::new();
    let mut nets = Vec::new();
    for (left, right) in pairs {
        let forward = format!("net-{left}-{right}");
        let reverse = format!("net-{right}-{left}");
        components.push(buffer(left, &reverse, &forward));
        components.push(buffer(right, &forward, &reverse));
        nets.push(Net {
            id: forward.clone(),
            name: forward.replace('-', "_"),
            width: 1,
        });
        nets.push(Net {
            id: reverse.clone(),
            name: reverse.replace('-', "_"),
            width: 1,
        });
    }
    document("cycle-graph", "cycle_graph", Vec::new(), components, nets)
}

fn large_acyclic(component_count: usize) -> CircuitDocument {
    let mut components = Vec::with_capacity(component_count);
    let mut nets = Vec::with_capacity(component_count + 1);
    for index in 0..=component_count {
        nets.push(Net {
            id: format!("n{index}"),
            name: format!("n{index}"),
            width: 1,
        });
    }
    for index in 0..component_count {
        components.push(buffer(
            &format!("b{index}"),
            &format!("n{index}"),
            &format!("n{}", index + 1),
        ));
    }
    let ports = vec![
        ModulePort {
            id: "input".into(),
            name: "data_in".into(),
            direction: PortDirection::Input,
            width: 1,
            net_id: "n0".into(),
        },
        ModulePort {
            id: "output".into(),
            name: "data_out".into(),
            direction: PortDirection::Output,
            width: 1,
            net_id: format!("n{component_count}"),
        },
    ];
    document("large-acyclic", "large_acyclic", ports, components, nets)
}

fn buffer(id: &str, input_net: &str, output_net: &str) -> Component {
    Component {
        id: id.into(),
        name: id.replace('-', "_"),
        component_type: ComponentType::Buffer,
        width: 1,
        connections: BTreeMap::from([
            ("A".into(), input_net.into()),
            ("Y".into(), output_net.into()),
        ]),
        parameters: Parameters::new(),
    }
}

fn document(
    id: &str,
    name: &str,
    ports: Vec<ModulePort>,
    components: Vec<Component>,
    nets: Vec<Net>,
) -> CircuitDocument {
    CircuitDocument {
        schema_version: SchemaVersion::new("1.0"),
        circuit: Circuit {
            id: id.into(),
            name: name.into(),
            ports,
            components,
            nets,
        },
        editor_metadata: None,
    }
}

fn first(report: &ValidationReport, code: DiagnosticCode) -> &Diagnostic {
    report
        .diagnostics()
        .iter()
        .find(|item| item.code == code)
        .unwrap_or_else(|| panic!("missing {code}: {:#?}", report.diagnostics()))
}

fn count(report: &ValidationReport, code: DiagnosticCode) -> usize {
    report
        .diagnostics()
        .iter()
        .filter(|item| item.code == code)
        .count()
}

fn assert_code(report: &ValidationReport, code: DiagnosticCode) {
    let _ = first(report, code);
}

fn assert_error_source(
    report: &ValidationReport,
    code: DiagnosticCode,
    component_id: Option<&str>,
    net_id: Option<&str>,
    port_id: Option<&str>,
    field: &str,
) {
    let diagnostic = first(report, code);
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.source.component_id.as_deref(), component_id);
    assert_eq!(diagnostic.source.net_id.as_deref(), net_id);
    assert_eq!(diagnostic.source.port_id.as_deref(), port_id);
    assert_eq!(diagnostic.source.field.as_deref(), Some(field));
    assert!(diagnostic.source.circuit_id.is_some());
}

fn next_u64(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    *state
}
