//! Lowers a flat NAND [`FlatNetlist`] into a canonical [`CircuitDocument`].
//!
//! Each net becomes a canonical `Net`, each NAND a `NAND` component, and each
//! boundary pin a module `ModulePort` carrying the pin's full width. Gates are
//! always one bit, so a connection into a multi-bit net is emitted as a schema
//! v1.1 slice. Where an output-pin bit is already driven under another name —
//! a module input wired straight through, say — a one-bit `BUFFER` copies it,
//! so every output bit has a component driver.

use std::collections::{BTreeMap, BTreeSet};

use jsonrtl::{
    Circuit, CircuitDocument, Component, ComponentType, Connection, ModulePort, Net, NetSlice,
    PortDirection, SchemaVersion,
};

use crate::dls::elaborate::{BitRef, FlatNetlist};

const SCHEMA_VERSION: &str = "1.1";
const GATE_WIDTH: u32 = 1;

fn net_id(index: usize) -> String {
    format!("net{index}")
}

/// Addresses one bit, using a bare net id when the net is only one bit wide so
/// single-bit chips still produce plain v1.0-shaped connections.
fn connection(bit: BitRef, net_widths: &[u32]) -> Connection {
    if net_widths.get(bit.net).copied().unwrap_or(1) == 1 {
        Connection::Whole(net_id(bit.net))
    } else {
        Connection::Slice(NetSlice {
            net: net_id(bit.net),
            msb: bit.bit,
            lsb: bit.bit,
        })
    }
}

/// Returns a module-unique port name, since the kernel requires distinct
/// external port names (DLS lets several pins share a name, e.g. two `IN`s).
fn unique_name(used: &mut BTreeSet<String>, base: &str) -> String {
    let base = if base.is_empty() { "port" } else { base };
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

/// Builds a canonical document for `chip_name` from its flattened netlist.
#[must_use]
pub fn lower(chip_name: &str, flat: &FlatNetlist) -> CircuitDocument {
    let widths = &flat.net_widths;

    let nets: Vec<Net> = widths
        .iter()
        .enumerate()
        .map(|(index, width)| Net {
            id: net_id(index),
            name: net_id(index),
            width: *width,
        })
        .collect();

    let mut components: Vec<Component> = flat
        .nands
        .iter()
        .enumerate()
        .map(|(index, nand)| Component {
            id: format!("gate{index}"),
            name: format!("gate{index}"),
            component_type: ComponentType::Nand,
            width: GATE_WIDTH,
            connections: BTreeMap::from([
                ("A".to_string(), connection(nand.a, widths)),
                ("B".to_string(), connection(nand.b, widths)),
                ("Y".to_string(), connection(nand.y, widths)),
            ]),
            parameters: BTreeMap::new(),
        })
        .collect();

    for (index, pass) in flat.buffers.iter().enumerate() {
        components.push(Component {
            id: format!("buffer{index}"),
            name: format!("buffer{index}"),
            component_type: ComponentType::Buffer,
            width: GATE_WIDTH,
            connections: BTreeMap::from([
                ("A".to_string(), connection(pass.from, widths)),
                ("Y".to_string(), connection(pass.to, widths)),
            ]),
            parameters: BTreeMap::new(),
        });
    }

    let mut used_names: BTreeSet<String> = BTreeSet::new();
    let mut ports: Vec<ModulePort> = flat
        .inputs
        .iter()
        .enumerate()
        .map(|(index, pin)| ModulePort {
            id: format!("port_in{index}"),
            name: unique_name(&mut used_names, &pin.name),
            direction: PortDirection::Input,
            width: pin.width,
            net_id: net_id(pin.net),
        })
        .collect();

    for (index, pin) in flat.outputs.iter().enumerate() {
        ports.push(ModulePort {
            id: format!("port_out{index}"),
            name: unique_name(&mut used_names, &pin.name),
            direction: PortDirection::Output,
            width: pin.width,
            net_id: net_id(pin.net),
        });
    }

    CircuitDocument {
        schema_version: SchemaVersion::new(SCHEMA_VERSION),
        circuit: Circuit {
            id: format!("dls-{chip_name}"),
            name: chip_name.to_string(),
            ports,
            components,
            nets,
        },
        editor_metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dls::elaborate::{BoundaryPin, NandInst, PassThrough, elaborate};
    use crate::dls::model::load_project;
    use jsonrtl::{CompileOptions, Kernel};
    use std::path::PathBuf;

    fn project_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dls/test")
    }

    fn compile(document: &CircuitDocument) -> jsonrtl::CompileResult {
        Kernel::default().compile_verilog(document, &CompileOptions::default())
    }

    #[test]
    fn and_lowers_and_compiles() {
        let project = load_project(&project_dir()).expect("load");
        let flat = elaborate(&project, "AND").expect("elaborate");
        let document = lower("AND", &flat);

        let report = Kernel::default().validate(&document);
        assert!(!report.has_errors(), "validation: {report:?}");

        let result = compile(&document);
        assert!(result.has_output(), "diagnostics: {:?}", result.diagnostics);
        let verilog = result.verilog.expect("verilog");
        // Two NAND assigns of the form `~(x & y)`.
        assert_eq!(verilog.matches("& ").count(), 2, "verilog was:\n{verilog}");
        assert_eq!(verilog.matches('~').count(), 2, "verilog was:\n{verilog}");
    }

    #[test]
    fn a_single_bit_chip_still_uses_whole_net_connections() {
        // Slices should appear only where a net is actually wide, so existing
        // single-bit output stays byte-identical apart from the version.
        let project = load_project(&project_dir()).expect("load");
        let document = lower("AND", &elaborate(&project, "AND").expect("elaborate"));
        assert!(
            document
                .circuit
                .components
                .iter()
                .flat_map(|component| component.connections.values())
                .all(|connection| connection.slice().is_none())
        );
    }

    #[test]
    fn one_bit_adder_lowers_and_compiles() {
        let project = load_project(&project_dir()).expect("load");
        let flat = elaborate(&project, "1-bit adder").expect("elaborate");
        let document = lower("1-bit adder", &flat);
        let result = compile(&document);
        assert!(result.has_output(), "diagnostics: {:?}", result.diagnostics);
    }

    #[test]
    fn passthrough_inserts_a_buffer_and_compiles() {
        // A synthetic chip: single input wired straight to a single output.
        let flat = FlatNetlist {
            inputs: vec![BoundaryPin {
                name: "a".into(),
                net: 0,
                width: 1,
            }],
            outputs: vec![BoundaryPin {
                name: "y".into(),
                net: 1,
                width: 1,
            }],
            nands: Vec::<NandInst>::new(),
            buffers: vec![PassThrough {
                from: BitRef { net: 0, bit: 0 },
                to: BitRef { net: 1, bit: 0 },
            }],
            net_widths: vec![1, 1],
        };
        let document = lower("wire", &flat);
        assert!(
            document
                .circuit
                .components
                .iter()
                .any(|component| component.component_type == ComponentType::Buffer),
            "expected a BUFFER for the pass-through"
        );
        let result = compile(&document);
        assert!(result.has_output(), "diagnostics: {:?}", result.diagnostics);
    }

    #[test]
    fn a_wide_port_keeps_its_bus_and_gates_address_single_bits() {
        // Two 4-bit ports, with one NAND driving bit 2 of the output.
        let flat = FlatNetlist {
            inputs: vec![BoundaryPin {
                name: "a".into(),
                net: 0,
                width: 4,
            }],
            outputs: vec![BoundaryPin {
                name: "y".into(),
                net: 1,
                width: 4,
            }],
            nands: (0..4)
                .map(|bit| NandInst {
                    a: BitRef { net: 0, bit },
                    b: BitRef { net: 0, bit },
                    y: BitRef { net: 1, bit },
                })
                .collect(),
            buffers: Vec::new(),
            net_widths: vec![4, 4],
        };
        let document = lower("nibble", &flat);
        assert_eq!(document.circuit.ports[0].width, 4);
        let verilog = compile(&document).verilog.expect("compiles");
        assert!(verilog.contains("input wire [3:0] a;"), "{verilog}");
        assert!(verilog.contains("output wire [3:0] y;"), "{verilog}");
        assert!(
            verilog.contains("assign net1[2] = ~(net0[2] & net0[2]);"),
            "{verilog}"
        );
    }
}
