use std::{collections::BTreeMap, sync::mpsc, time::Duration};

use jsonrtl::{
    CircuitDocument, CompileOptions, Component, ComponentType, Connection, DiagnosticCode, Kernel,
    NetSlice, SchemaVersion,
};
use serde_json::json;

fn shifted_bus() -> CircuitDocument {
    CircuitDocument::from_json(
        &json!({
            "schemaVersion": "1.1",
            "circuit": {
                "id": "shift", "name": "shift",
                "ports": [{"id": "out", "name": "out", "direction": "output",
                    "width": 3, "netId": "bus"}],
                "nets": [{"id": "bus", "name": "bus", "width": 3}],
                "components": [
                    {"id": "seed", "name": "seed", "type": "CONST", "width": 1,
                        "connections": {"Y": {"net": "bus", "msb": 0, "lsb": 0}},
                        "parameters": {"value": "1"}},
                    {"id": "shift", "name": "shift", "type": "BUFFER", "width": 2,
                        "connections": {"A": {"net": "bus", "msb": 1, "lsb": 0},
                            "Y": {"net": "bus", "msb": 2, "lsb": 1}},
                        "parameters": {}}
                ]
            }
        })
        .to_string(),
    )
    .unwrap()
}

#[test]
fn overlapping_shifted_bus_slices_compile_without_a_false_cycle() {
    let document = shifted_bus();
    let result = Kernel::default().compile_verilog(&document, &CompileOptions::default());
    assert!(!result.diagnostics.has_errors(), "{:?}", result.diagnostics);
    let verilog = result.verilog.unwrap();
    assert!(verilog.contains("assign bus[2:1] = bus[1:0];"), "{verilog}");
}

#[test]
fn different_lanes_can_pass_through_a_component_in_opposite_directions() {
    let document = CircuitDocument::from_json(
        &json!({"schemaVersion": "1.1", "circuit": {
            "id": "lanes", "name": "lanes", "ports": [],
            "nets": [{"id": "a", "name": "a", "width": 2},
                     {"id": "b", "name": "b", "width": 2}],
            "components": [
                {"id": "seed", "name": "seed", "type": "CONST", "width": 1,
                 "connections": {"Y": {"net": "a", "msb": 0, "lsb": 0}},
                 "parameters": {"value": "1"}},
                {"id": "wide", "name": "wide", "type": "BUFFER", "width": 2,
                 "connections": {"A": "a", "Y": "b"}, "parameters": {}},
                {"id": "narrow", "name": "narrow", "type": "BUFFER", "width": 1,
                 "connections": {"A": {"net": "b", "msb": 0, "lsb": 0},
                                 "Y": {"net": "a", "msb": 1, "lsb": 1}},
                 "parameters": {}}
            ]
        }})
        .to_string(),
    )
    .unwrap();
    let kernel = Kernel::default();
    let result = kernel.compile_verilog(&document, &CompileOptions::default());
    assert!(result.has_output(), "{:?}", result.diagnostics);
    let mut permuted = document.clone();
    permuted.circuit.components.reverse();
    permuted.circuit.nets.reverse();
    assert_eq!(
        result,
        kernel.compile_verilog(&permuted, &CompileOptions::default())
    );
}

#[test]
fn typed_slice_extremes_return_errors_instead_of_panicking() {
    for (msb, lsb, net_width) in [(u32::MAX, 0, 3), (u32::MAX, u32::MAX, 3), (0, 0, 0)] {
        let mut document = shifted_bus();
        document.circuit.nets[0].width = net_width;
        document.circuit.components[1].connections.insert(
            "A".into(),
            Connection::Slice(NetSlice {
                net: "bus".into(),
                msb,
                lsb,
            }),
        );
        let result = Kernel::default().compile_verilog(&document, &CompileOptions::default());
        assert!(!result.has_output());
        assert!(
            result
                .diagnostics
                .diagnostics()
                .iter()
                .any(|diagnostic| { diagnostic.code == DiagnosticCode::SliceOutOfRange })
        );
    }
    assert_eq!(
        NetSlice {
            net: "bus".into(),
            msb: u32::MAX,
            lsb: 0
        }
        .width(),
        None
    );
    assert_eq!(
        NetSlice {
            net: "bus".into(),
            msb: u32::MAX,
            lsb: u32::MAX
        }
        .width(),
        Some(1)
    );
}

#[test]
fn typed_unknown_schema_versions_never_compile() {
    let input = include_str!("../../../tests/fixtures/valid/minimal-and.json");
    for version in ["9.9", "1.10", "", "01.1"] {
        let mut document = CircuitDocument::from_json(input).unwrap();
        document.schema_version = SchemaVersion::new(version);
        let result = Kernel::default().compile_verilog(&document, &CompileOptions::default());
        assert!(result.diagnostics.has_errors(), "accepted {version}");
        assert!(!result.has_output());
    }
}

#[test]
fn thousands_of_conflicting_drivers_finish_with_one_complete_diagnostic() {
    let mut document = shifted_bus();
    document.circuit.ports.clear();
    document.circuit.nets[0].width = 1;
    document.circuit.components = (0..2_000)
        .map(|index| Component {
            id: format!("c{index:04}"),
            name: format!("c{index:04}"),
            component_type: ComponentType::Const,
            width: 1,
            connections: BTreeMap::from([("Y".into(), "bus".into())]),
            parameters: BTreeMap::from([("value".into(), json!("0"))]),
        })
        .collect();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        sender.send(Kernel::default().validate(&document)).unwrap();
    });
    let report = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("multiple-driver validation must finish within five seconds");
    let errors: Vec<_> = report.errors().collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, DiagnosticCode::NetMultipleDrivers);
    assert_eq!(errors[0].related_sources.len(), 2_000);
}

#[test]
fn bus_cycle_detection_agrees_with_an_independent_bit_graph() {
    let mut state = 0x4a17_u64;
    for case in 0..128 {
        let mut components = Vec::new();
        let mut edges = vec![Vec::new(); 16];
        for net in 0..4 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let output_start = ((state >> 32) % 2 * 2) as usize;
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let input_net = ((state >> 32) % 4) as usize;
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let input_start = ((state >> 32) % 3) as usize;
            components.push(json!({
                "id": format!("gate{net}"), "name": format!("gate{net}"),
                "type": "BUFFER", "width": 2, "parameters": {},
                "connections": {
                    "A": {"net": format!("n{input_net}"), "lsb": input_start, "msb": input_start + 1},
                    "Y": {"net": format!("n{net}"), "lsb": output_start, "msb": output_start + 1}
                }
            }));
            let constant_start = 2 - output_start;
            components.push(json!({
                "id": format!("seed{net}"), "name": format!("seed{net}"),
                "type": "CONST", "width": 2, "parameters": {"value": "01"},
                "connections": {"Y": {"net": format!("n{net}"), "lsb": constant_start, "msb": constant_start + 1}}
            }));
            for lane in 0..2 {
                edges[net * 4 + output_start + lane].push(input_net * 4 + input_start + lane);
            }
        }
        // Kahn's algorithm on individual net bits is deliberately independent
        // of the kernel's component graph and DFS candidate refinement.
        let mut incoming = [0_usize; 16];
        for targets in &edges {
            for target in targets {
                incoming[*target] += 1;
            }
        }
        let mut ready: Vec<_> = (0..16).filter(|node| incoming[*node] == 0).collect();
        let mut visited = 0;
        while let Some(node) = ready.pop() {
            visited += 1;
            for target in &edges[node] {
                incoming[*target] -= 1;
                if incoming[*target] == 0 {
                    ready.push(*target);
                }
            }
        }
        let expected_cycle = visited != 16;
        let nets: Vec<_> = (0..4)
            .map(|net| json!({"id": format!("n{net}"), "name": format!("n{net}"), "width": 4}))
            .collect();
        let document = CircuitDocument::from_json(
            &json!({"schemaVersion": "1.1", "circuit": {
                "id": "graph", "name": "graph", "ports": [], "components": components, "nets": nets
            }})
            .to_string(),
        )
        .unwrap();
        let report = Kernel::default().validate(&document);
        let actual_cycle = report
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::GraphCombinationalCycle);
        assert_eq!(
            actual_cycle, expected_cycle,
            "case {case}: {document:?}\n{report:?}"
        );
        let mut permuted = document.clone();
        permuted.circuit.components.reverse();
        permuted.circuit.nets.reverse();
        assert_eq!(report, Kernel::default().validate(&permuted), "case {case}");
    }
}

#[test]
fn driver_sweep_reports_exactly_the_conflicting_endpoints() {
    for ranges in [
        vec![(0, 8), (1, 2), (3, 4), (8, 9)],
        vec![(0, 2), (2, 4), (4, 6)],
        vec![(0, 2), (1, 3), (4, 6), (5, 7), (9, 10)],
        vec![(0, 10), (0, 1), (0, 5), (2, 9), (9, 10)],
    ] {
        let mut document = shifted_bus();
        document.circuit.ports.clear();
        document.circuit.nets[0].width = 10;
        document.circuit.components = ranges
            .iter()
            .enumerate()
            .map(|(index, (start, end))| Component {
                id: format!("c{index}"),
                name: format!("c{index}"),
                component_type: ComponentType::Const,
                width: end - start,
                connections: BTreeMap::from([(
                    "Y".into(),
                    Connection::Slice(NetSlice {
                        net: "bus".into(),
                        msb: end - 1,
                        lsb: *start,
                    }),
                )]),
                parameters: BTreeMap::from([(
                    "value".into(),
                    json!("0".repeat((end - start) as usize)),
                )]),
            })
            .collect();
        let expected: std::collections::BTreeSet<_> = ranges
            .iter()
            .enumerate()
            .filter(|(index, (start, end))| {
                ranges
                    .iter()
                    .enumerate()
                    .any(|(other, (other_start, other_end))| {
                        other != *index && start < other_end && other_start < end
                    })
            })
            .map(|(index, _)| format!("c{index}"))
            .collect();
        let report = Kernel::default().validate(&document);
        let actual: std::collections::BTreeSet<_> = report
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::NetMultipleDrivers)
            .flat_map(|diagnostic| &diagnostic.related_sources)
            .filter_map(|source| source.component_id.clone())
            .collect();
        assert_eq!(actual, expected, "{ranges:?}");
        assert_eq!(report.errors().count(), usize::from(!expected.is_empty()));
    }
}
