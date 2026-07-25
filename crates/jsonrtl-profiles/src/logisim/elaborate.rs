//! Turns a Logisim circuit into a flat gate netlist.
//!
//! Connectivity is geometric. Every wire segment, and every component port
//! position from [`crate::logisim::geometry`], is reduced to a point; points
//! that touch are merged into one net. Logisim also joins a wire endpoint that
//! lands part-way along another wire, so segments are tested against every
//! known point, not just against each other's endpoints.

use std::collections::BTreeMap;

use crate::{
    ProfileError,
    logisim::{
        geometry::{PortRole, gate_ports, pin_port, unary_ports},
        model::{Comp, LogisimProject, Point},
    },
};

/// A gate the kernel can express, before arity decomposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateKind {
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Xnor,
    Not,
    Buffer,
}

/// One gate instance wired to flat net indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateInst {
    pub kind: GateKind,
    pub inputs: Vec<usize>,
    pub output: usize,
}

/// A module-boundary pin bound to a flat net index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryPin {
    pub name: String,
    pub net: usize,
}

/// A circuit flattened to gates over a dense range of net indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatNetlist {
    pub inputs: Vec<BoundaryPin>,
    pub outputs: Vec<BoundaryPin>,
    pub gates: Vec<GateInst>,
    pub net_count: usize,
}

/// What a `<comp>` maps onto.
enum Kind {
    Gate(GateKind),
    /// A Pin; true when it reads the sheet (a module output).
    Pin(bool),
    Subcircuit,
}

/// Classifies a component, rejecting anything outside the supported subset.
fn classify(comp: &Comp, circuit: &str) -> Result<Kind, ProfileError> {
    // A component with no library reference is an instance of another circuit.
    if comp.lib.is_none() {
        return Ok(Kind::Subcircuit);
    }
    let kind = match comp.name.as_str() {
        "AND Gate" => Kind::Gate(GateKind::And),
        "OR Gate" => Kind::Gate(GateKind::Or),
        "XOR Gate" => Kind::Gate(GateKind::Xor),
        "NAND Gate" => Kind::Gate(GateKind::Nand),
        "NOR Gate" => Kind::Gate(GateKind::Nor),
        "XNOR Gate" => Kind::Gate(GateKind::Xnor),
        "NOT Gate" => Kind::Gate(GateKind::Not),
        "Buffer" => Kind::Gate(GateKind::Buffer),
        "Pin" => Kind::Pin(comp.attr_bool("output")),
        other => {
            return Err(ProfileError::Unsupported {
                chip: circuit.to_string(),
                detail: format!(
                    "component '{other}' at {} is outside the supported subset (pins, basic gates, and subcircuits)",
                    comp.loc
                ),
            });
        }
    };
    Ok(kind)
}

/// Rejects any component carrying a bit width other than one.
fn check_single_bit(comp: &Comp, circuit: &str) -> Result<(), ProfileError> {
    if let Some(width) = comp.attr_int("width")
        && width != 1
    {
        return Err(ProfileError::Unsupported {
            chip: circuit.to_string(),
            detail: format!(
                "component '{}' at {} is {width} bits wide; only single-bit signals are supported",
                comp.name, comp.loc
            ),
        });
    }
    Ok(())
}

/// True when `point` lies on the axis-aligned segment `from`-`to`.
fn on_segment(point: Point, from: Point, to: Point) -> bool {
    let within = |value: i64, a: i64, b: i64| value >= a.min(b) && value <= a.max(b);
    if from.x == to.x {
        point.x == from.x && within(point.y, from.y, to.y)
    } else if from.y == to.y {
        point.y == from.y && within(point.x, from.x, to.x)
    } else {
        // Logisim wires are axis-aligned; a diagonal only connects at its ends.
        (point == from) || (point == to)
    }
}

/// Flattens `circuit_name` within `project`.
pub fn elaborate(
    project: &LogisimProject,
    circuit_name: &str,
) -> Result<FlatNetlist, ProfileError> {
    let circuit = project
        .circuits
        .get(circuit_name)
        .ok_or_else(|| ProfileError::Structure {
            chip: circuit_name.to_string(),
            detail: format!("no circuit named '{circuit_name}' in project"),
        })?;

    // Classify first so an unsupported component is reported before any work.
    let mut kinds = Vec::with_capacity(circuit.comps.len());
    for comp in &circuit.comps {
        check_single_bit(comp, circuit_name)?;
        let kind = classify(comp, circuit_name)?;
        if matches!(kind, Kind::Subcircuit) {
            // Subcircuit port geometry depends on the instance's appearance,
            // which cannot be reconstructed reliably without a reference file.
            // Refuse rather than guess and mis-wire the circuit silently.
            return Err(ProfileError::Unsupported {
                chip: circuit_name.to_string(),
                detail: format!(
                    "subcircuit '{}' at {}: instance port geometry is not calibrated yet, so its wiring cannot be resolved safely",
                    comp.name, comp.loc
                ),
            });
        }
        kinds.push(kind);
    }

    // Collect every port position alongside its owning component.
    let mut ports: Vec<(usize, PortRole, Point)> = Vec::new();
    for (index, (comp, kind)) in circuit.comps.iter().zip(kinds.iter()).enumerate() {
        match kind {
            Kind::Gate(GateKind::Not | GateKind::Buffer) => {
                for port in unary_ports(comp) {
                    ports.push((index, port.role, port.point));
                }
            }
            Kind::Gate(_) => {
                let inputs = comp.attr_int("inputs").unwrap_or(2).max(1) as usize;
                for port in gate_ports(comp, inputs) {
                    ports.push((index, port.role, port.point));
                }
            }
            Kind::Pin(is_output) => {
                let port = pin_port(comp, *is_output);
                ports.push((index, port.role, port.point));
            }
            Kind::Subcircuit => unreachable!("rejected above"),
        }
    }

    // Union-find over every distinct point that matters.
    let mut node_of: BTreeMap<Point, usize> = BTreeMap::new();
    let mut uf = UnionFind::default();
    let intern = |point: Point, uf: &mut UnionFind, node_of: &mut BTreeMap<Point, usize>| {
        *node_of.entry(point).or_insert_with(|| uf.make())
    };
    for (_, _, point) in &ports {
        intern(*point, &mut uf, &mut node_of);
    }
    for wire in &circuit.wires {
        intern(wire.from, &mut uf, &mut node_of);
        intern(wire.to, &mut uf, &mut node_of);
    }

    // Merge each segment's own ends, then anything lying along it.
    let points: Vec<Point> = node_of.keys().copied().collect();
    for wire in &circuit.wires {
        let anchor = node_of[&wire.from];
        uf.union(anchor, node_of[&wire.to]);
        for point in &points {
            if on_segment(*point, wire.from, wire.to) {
                uf.union(anchor, node_of[point]);
            }
        }
    }

    // Dense net numbering in a stable order.
    let mut net_of_root: BTreeMap<usize, usize> = BTreeMap::new();
    let mut net_for = |point: Point, uf: &mut UnionFind| {
        let root = uf.find(node_of[&point]);
        let next = net_of_root.len();
        *net_of_root.entry(root).or_insert(next)
    };

    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut gates = Vec::new();
    for (index, (comp, kind)) in circuit.comps.iter().zip(kinds.iter()).enumerate() {
        match kind {
            Kind::Pin(is_output) => {
                let point = ports
                    .iter()
                    .find(|(owner, _, _)| *owner == index)
                    .map(|(_, _, point)| *point)
                    .expect("pins always contribute a port");
                let pin = BoundaryPin {
                    name: comp
                        .attr("label")
                        .filter(|label| !label.trim().is_empty())
                        .unwrap_or(if *is_output { "out" } else { "in" })
                        .to_string(),
                    net: net_for(point, &mut uf),
                };
                if *is_output {
                    outputs.push(pin)
                } else {
                    inputs.push(pin)
                }
            }
            Kind::Gate(gate) => {
                let mut owned: Vec<(PortRole, Point)> = ports
                    .iter()
                    .filter(|(owner, _, _)| *owner == index)
                    .map(|(_, role, point)| (*role, *point))
                    .collect();
                owned.sort_by_key(|(role, _)| match role {
                    PortRole::Output => usize::MAX,
                    PortRole::Input(slot) => *slot,
                });
                let output = owned
                    .iter()
                    .find(|(role, _)| matches!(role, PortRole::Output))
                    .map(|(_, point)| *point)
                    .expect("every gate has an output");
                let gate_inputs: Vec<usize> = owned
                    .iter()
                    .filter(|(role, _)| matches!(role, PortRole::Input(_)))
                    .map(|(_, point)| net_for(*point, &mut uf))
                    .collect();
                gates.push(GateInst {
                    kind: *gate,
                    inputs: gate_inputs,
                    output: net_for(output, &mut uf),
                });
            }
            Kind::Subcircuit => unreachable!("rejected above"),
        }
    }

    Ok(FlatNetlist {
        inputs,
        outputs,
        gates,
        net_count: net_of_root.len(),
    })
}

#[derive(Default)]
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn make(&mut self) -> usize {
        let id = self.parent.len();
        self.parent.push(id);
        id
    }

    fn find(&mut self, mut node: usize) -> usize {
        while self.parent[node] != node {
            self.parent[node] = self.parent[self.parent[node]];
            node = self.parent[node];
        }
        node
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[rb] = ra;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logisim::model::parse_project;

    /// An AND gate at (200,100) fed by two pins, output to a third pin.
    /// Input ports land at (170,90) and (170,110) under the geometry rules.
    const AND_CIRCUIT: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<project source="3.8.0" version="1.0">
  <lib desc="#Wiring" name="0"/>
  <lib desc="#Gates" name="1"/>
  <main name="main"/>
  <circuit name="main">
    <comp lib="0" loc="(100,90)" name="Pin"><a name="label" val="a"/></comp>
    <comp lib="0" loc="(100,110)" name="Pin"><a name="label" val="b"/></comp>
    <comp lib="0" loc="(300,100)" name="Pin">
      <a name="label" val="y"/><a name="output" val="true"/>
    </comp>
    <comp lib="1" loc="(200,100)" name="AND Gate"><a name="inputs" val="2"/></comp>
    <wire from="(100,90)" to="(170,90)"/>
    <wire from="(100,110)" to="(170,110)"/>
    <wire from="(200,100)" to="(300,100)"/>
  </circuit>
</project>"##;

    #[test]
    fn resolves_pins_and_a_gate_through_coordinates() {
        let project = parse_project(AND_CIRCUIT, "t").expect("parse");
        let flat = elaborate(&project, "main").expect("elaborate");

        assert_eq!(flat.inputs.len(), 2);
        assert_eq!(flat.outputs.len(), 1);
        assert_eq!(flat.gates.len(), 1);

        let gate = &flat.gates[0];
        assert_eq!(gate.kind, GateKind::And);
        // Each gate input shares a net with the pin wired to it.
        let input_nets: Vec<usize> = flat.inputs.iter().map(|pin| pin.net).collect();
        assert_eq!(gate.inputs.len(), 2);
        for net in &gate.inputs {
            assert!(
                input_nets.contains(net),
                "gate input {net} is not a pin net"
            );
        }
        // The gate output shares a net with the output pin.
        assert_eq!(gate.output, flat.outputs[0].net);
    }

    #[test]
    fn a_wire_ending_mid_segment_still_connects() {
        // A vertical stub meets a horizontal run part-way along it.
        let circuit = AND_CIRCUIT.replace(
            r#"<wire from="(200,100)" to="(300,100)"/>"#,
            r#"<wire from="(200,100)" to="(300,100)"/><wire from="(250,100)" to="(250,200)"/>"#,
        );
        let project = parse_project(&circuit, "t").expect("parse");
        let flat = elaborate(&project, "main").expect("elaborate");
        // The stub joins the output net rather than forming a separate one.
        assert_eq!(flat.gates[0].output, flat.outputs[0].net);
    }

    #[test]
    fn rejects_unsupported_components() {
        let circuit = AND_CIRCUIT.replace(r#"name="AND Gate""#, r#"name="Multiplexer""#);
        let project = parse_project(&circuit, "t").expect("parse");
        match elaborate(&project, "main").unwrap_err() {
            ProfileError::Unsupported { detail, .. } => {
                assert!(detail.contains("Multiplexer"), "{detail}")
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn rejects_multi_bit_components() {
        let circuit = AND_CIRCUIT.replace(
            r#"<a name="label" val="a"/>"#,
            r#"<a name="label" val="a"/><a name="width" val="8"/>"#,
        );
        let project = parse_project(&circuit, "t").expect("parse");
        match elaborate(&project, "main").unwrap_err() {
            ProfileError::Unsupported { detail, .. } => {
                assert!(detail.contains("8 bits wide"), "{detail}")
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn rejects_subcircuits_until_geometry_is_calibrated() {
        let circuit = AND_CIRCUIT.replace(
            r#"<comp lib="1" loc="(200,100)" name="AND Gate"><a name="inputs" val="2"/></comp>"#,
            r#"<comp loc="(200,100)" name="helper"/>"#,
        );
        let project = parse_project(&circuit, "t").expect("parse");
        match elaborate(&project, "main").unwrap_err() {
            ProfileError::Unsupported { detail, .. } => {
                assert!(detail.contains("not calibrated"), "{detail}")
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }
}
