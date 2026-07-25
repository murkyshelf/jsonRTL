//! Lowers a flat Logisim netlist into a canonical [`CircuitDocument`].
//!
//! Logisim gates take up to 32 inputs; the canonical catalog is 2-input, so a
//! wider gate folds into a balanced-left chain of native gates. Inverting gates
//! fold on their non-inverting base and invert once at the end, which is what
//! Logisim means by an n-input NAND.

use std::collections::{BTreeMap, BTreeSet};

use jsonrtl::{
    Circuit, CircuitDocument, Component, ComponentType, ModulePort, Net, PortDirection,
    SchemaVersion,
};

use crate::logisim::elaborate::{FlatNetlist, GateInst, GateKind};

const SCHEMA_VERSION: &str = "1.0";
const WIDTH: u32 = 1;

fn net_id(index: usize) -> String {
    format!("net{index}")
}

/// Returns a module-unique port name; Logisim allows duplicate pin labels but
/// the kernel requires distinct external port names.
fn unique_name(used: &mut BTreeSet<String>, base: &str) -> String {
    let base = if base.trim().is_empty() { "port" } else { base };
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}_{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

/// The non-inverting base a gate folds on, and whether the fold is inverted.
fn fold_base(kind: GateKind) -> (ComponentType, bool) {
    match kind {
        GateKind::And => (ComponentType::And, false),
        GateKind::Or => (ComponentType::Or, false),
        GateKind::Xor => (ComponentType::Xor, false),
        GateKind::Nand => (ComponentType::And, true),
        GateKind::Nor => (ComponentType::Or, true),
        GateKind::Xnor => (ComponentType::Xor, true),
        GateKind::Not => (ComponentType::Not, false),
        GateKind::Buffer => (ComponentType::Buffer, false),
    }
}

/// The native 2-input catalog type for a gate, when one exists.
fn native(kind: GateKind) -> Option<ComponentType> {
    Some(match kind {
        GateKind::And => ComponentType::And,
        GateKind::Or => ComponentType::Or,
        GateKind::Xor => ComponentType::Xor,
        GateKind::Nand => ComponentType::Nand,
        GateKind::Nor => ComponentType::Nor,
        GateKind::Xnor => ComponentType::Xnor,
        _ => return None,
    })
}

struct Builder {
    components: Vec<Component>,
    nets: Vec<Net>,
    next_net: usize,
}

impl Builder {
    fn fresh_net(&mut self) -> String {
        let id = net_id(self.next_net);
        self.next_net += 1;
        self.nets.push(Net {
            id: id.clone(),
            name: id.clone(),
            width: WIDTH,
        });
        id
    }

    fn push(
        &mut self,
        index: usize,
        suffix: &str,
        component_type: ComponentType,
        connections: ConnectionList,
    ) {
        let id = format!("gate{index}{suffix}");
        self.components.push(Component {
            id: id.clone(),
            name: id,
            component_type,
            width: WIDTH,
            connections: connections
                .into_iter()
                .map(|(port, net)| (port, net.into()))
                .collect(),
            parameters: BTreeMap::new(),
        });
    }
}

type ConnectionList = Vec<(String, String)>;

/// Emits one Logisim gate, decomposing it into native catalog gates.
fn emit_gate(builder: &mut Builder, index: usize, gate: &GateInst) {
    let output = net_id(gate.output);
    let inputs: Vec<String> = gate.inputs.iter().map(|net| net_id(*net)).collect();

    // Unary gates map straight across.
    if matches!(gate.kind, GateKind::Not | GateKind::Buffer) {
        let component_type = fold_base(gate.kind).0;
        let source = inputs.first().cloned().unwrap_or_else(|| output.clone());
        builder.push(
            index,
            "",
            component_type,
            vec![("A".into(), source), ("Y".into(), output)],
        );
        return;
    }

    match inputs.len() {
        // A one-input AND/OR/XOR passes its input through; the inverting forms
        // reduce to a plain inverter.
        0 | 1 => {
            let (_, inverted) = fold_base(gate.kind);
            let source = inputs.first().cloned().unwrap_or_else(|| output.clone());
            let component_type = if inverted {
                ComponentType::Not
            } else {
                ComponentType::Buffer
            };
            builder.push(
                index,
                "",
                component_type,
                vec![("A".into(), source), ("Y".into(), output)],
            );
        }
        // Two inputs use the native catalog entry, inverting forms included.
        2 => {
            let component_type = native(gate.kind).expect("binary gate has a native type");
            builder.push(
                index,
                "",
                component_type,
                vec![
                    ("A".into(), inputs[0].clone()),
                    ("B".into(), inputs[1].clone()),
                    ("Y".into(), output),
                ],
            );
        }
        // Wider gates fold pairwise on the non-inverting base.
        _ => {
            let (base, inverted) = fold_base(gate.kind);
            let mut accumulator = inputs[0].clone();
            for (step, operand) in inputs.iter().enumerate().skip(1) {
                let last = step == inputs.len() - 1;
                let target = if last && !inverted {
                    output.clone()
                } else {
                    builder.fresh_net()
                };
                builder.push(
                    index,
                    &format!("_f{step}"),
                    base,
                    vec![
                        ("A".into(), accumulator),
                        ("B".into(), operand.clone()),
                        ("Y".into(), target.clone()),
                    ],
                );
                accumulator = target;
            }
            if inverted {
                builder.push(
                    index,
                    "_inv",
                    ComponentType::Not,
                    vec![("A".into(), accumulator), ("Y".into(), output)],
                );
            }
        }
    }
}

/// Builds a canonical document for `circuit_name` from its flattened netlist.
#[must_use]
pub fn lower(circuit_name: &str, flat: &FlatNetlist) -> CircuitDocument {
    let mut builder = Builder {
        components: Vec::new(),
        nets: (0..flat.net_count)
            .map(|index| Net {
                id: net_id(index),
                name: net_id(index),
                width: WIDTH,
            })
            .collect(),
        next_net: flat.net_count,
    };

    for (index, gate) in flat.gates.iter().enumerate() {
        emit_gate(&mut builder, index, gate);
    }

    // Nets driven by a component can back an output port directly; anything
    // else (a pin wired straight to a pin) needs a buffer to have a driver.
    let driven: BTreeSet<String> = builder
        .components
        .iter()
        .filter_map(|component| {
            component
                .connections
                .get("Y")
                .map(|c| c.net_id().to_string())
        })
        .collect();

    let mut used_names = BTreeSet::new();
    let mut ports: Vec<ModulePort> = flat
        .inputs
        .iter()
        .enumerate()
        .map(|(index, pin)| ModulePort {
            id: format!("port_in{index}"),
            name: unique_name(&mut used_names, &pin.name),
            direction: PortDirection::Input,
            width: WIDTH,
            net_id: net_id(pin.net),
        })
        .collect();

    for (index, pin) in flat.outputs.iter().enumerate() {
        let source = net_id(pin.net);
        let net_for_port = if driven.contains(&source) {
            source
        } else {
            let buffered = builder.fresh_net();
            builder.components.push(Component {
                id: format!("buffer{index}"),
                name: format!("buffer{index}"),
                component_type: ComponentType::Buffer,
                width: WIDTH,
                connections: BTreeMap::from([
                    ("A".to_string(), source.into()),
                    ("Y".to_string(), buffered.clone().into()),
                ]),
                parameters: BTreeMap::new(),
            });
            buffered
        };
        ports.push(ModulePort {
            id: format!("port_out{index}"),
            name: unique_name(&mut used_names, &pin.name),
            direction: PortDirection::Output,
            width: WIDTH,
            net_id: net_for_port,
        });
    }

    CircuitDocument {
        schema_version: SchemaVersion::new(SCHEMA_VERSION),
        circuit: Circuit {
            id: format!("logisim-{circuit_name}"),
            name: circuit_name.to_string(),
            ports,
            components: builder.components,
            nets: builder.nets,
        },
        editor_metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logisim::elaborate::BoundaryPin;
    use jsonrtl::{CompileOptions, Kernel};

    fn compile(document: &CircuitDocument) -> String {
        let result = Kernel::default().compile_verilog(document, &CompileOptions::default());
        assert!(
            result.has_output(),
            "compile failed: {:?}",
            result.diagnostics
        );
        result.verilog.expect("verilog")
    }

    fn netlist(kind: GateKind, input_count: usize) -> FlatNetlist {
        let inputs: Vec<BoundaryPin> = (0..input_count)
            .map(|index| BoundaryPin {
                name: format!("i{index}"),
                net: index,
            })
            .collect();
        FlatNetlist {
            gates: vec![GateInst {
                kind,
                inputs: (0..input_count).collect(),
                output: input_count,
            }],
            outputs: vec![BoundaryPin {
                name: "y".into(),
                net: input_count,
            }],
            inputs,
            net_count: input_count + 1,
        }
    }

    #[test]
    fn two_input_gates_use_the_native_catalog_entry() {
        let verilog = compile(&lower("m", &netlist(GateKind::Nand, 2)));
        assert!(
            verilog.contains("~("),
            "expected an inverting assign:\n{verilog}"
        );
        assert_eq!(verilog.matches('&').count(), 1, "{verilog}");
    }

    #[test]
    fn wide_gates_fold_into_two_input_gates() {
        let document = lower("m", &netlist(GateKind::And, 4));
        // Three 2-input ANDs express a 4-input AND.
        assert_eq!(document.circuit.components.len(), 3);
        assert!(
            document
                .circuit
                .components
                .iter()
                .all(|component| component.component_type == ComponentType::And)
        );
        compile(&document);
    }

    #[test]
    fn wide_inverting_gates_invert_once_at_the_end() {
        let document = lower("m", &netlist(GateKind::Nand, 3));
        let types: Vec<ComponentType> = document
            .circuit
            .components
            .iter()
            .map(|component| component.component_type)
            .collect();
        assert_eq!(
            types
                .iter()
                .filter(|kind| **kind == ComponentType::And)
                .count(),
            2
        );
        assert_eq!(
            types
                .iter()
                .filter(|kind| **kind == ComponentType::Not)
                .count(),
            1
        );
        compile(&document);
    }

    #[test]
    fn a_pin_wired_straight_to_a_pin_gets_a_buffer() {
        let flat = FlatNetlist {
            inputs: vec![BoundaryPin {
                name: "a".into(),
                net: 0,
            }],
            outputs: vec![BoundaryPin {
                name: "y".into(),
                net: 0,
            }],
            gates: Vec::new(),
            net_count: 1,
        };
        let document = lower("m", &flat);
        assert!(
            document
                .circuit
                .components
                .iter()
                .any(|component| component.component_type == ComponentType::Buffer)
        );
        compile(&document);
    }

    #[test]
    fn duplicate_pin_labels_are_disambiguated() {
        let flat = FlatNetlist {
            inputs: vec![
                BoundaryPin {
                    name: "a".into(),
                    net: 0,
                },
                BoundaryPin {
                    name: "a".into(),
                    net: 1,
                },
            ],
            outputs: vec![BoundaryPin {
                name: "y".into(),
                net: 2,
            }],
            gates: vec![GateInst {
                kind: GateKind::Or,
                inputs: vec![0, 1],
                output: 2,
            }],
            net_count: 3,
        };
        let names: Vec<String> = lower("m", &flat)
            .circuit
            .ports
            .iter()
            .map(|port| port.name.clone())
            .collect();
        assert_eq!(names, vec!["a", "a_2", "y"]);
    }
}
