//! Flattens a hierarchical DLS chip into a flat netlist of NAND primitives.
//!
//! DLS chips compose other chips down to the built-in NAND (input pins `0`,`1`;
//! output pin `2`). Elaboration inlines every custom sub-chip recursively and
//! uses union-find over wire endpoints to coalesce connected pins into nets.
//!
//! The union-find works on **single bits**, not pins. That is what makes buses
//! cheap: a splitter or merger introduces no logic at all, it just says that
//! one of its narrow pins *is* a particular bit of its wide pin, so both sides
//! share a node. Only at the end are nodes grouped back into multi-bit nets.

use std::collections::BTreeMap;

use crate::{
    ProfileError,
    dls::builtin::{Builtin, piece_bits},
    dls::model::{ChipDef, DlsProject, PinAddress, PinDef},
};

/// Guards against pathological or cyclic chip nesting.
const MAX_DEPTH: usize = 256;

/// Guards against exponential inlining. Nesting is bounded by [`MAX_DEPTH`], but
/// a chip that instantiates its child twice doubles the instance count per
/// level, so depth alone is not a bound. This cap sits far above the kernel's
/// own component limit, so no compilable circuit is ever rejected by it, while
/// a runaway project fails fast instead of exhausting memory.
const MAX_INSTANCES: usize = 50_000;

/// Guards total bit-node allocation. Wide pins multiply node count by their
/// width, so an instance cap alone no longer bounds memory.
const MAX_NODES: usize = 2_000_000;

/// The built-in NAND primitive's fixed pin ids.
const NAND_IN_A: i64 = 0;
const NAND_IN_B: i64 = 1;
const NAND_OUT: i64 = 2;

/// One bit of one canonical net.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BitRef {
    pub net: usize,
    pub bit: u32,
}

/// A module-boundary pin bound to a whole canonical net.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryPin {
    pub name: String,
    pub net: usize,
    pub width: u32,
}

/// A NAND instance wired to single bits (`Y = ~(A & B)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NandInst {
    pub a: BitRef,
    pub b: BitRef,
    pub y: BitRef,
}

/// A one-bit pass-through, needed where an output-pin bit is already driven
/// somewhere else and so cannot simply share that net.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassThrough {
    pub from: BitRef,
    pub to: BitRef,
}

/// A chip flattened to NAND primitives over a dense range of net indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatNetlist {
    pub inputs: Vec<BoundaryPin>,
    pub outputs: Vec<BoundaryPin>,
    pub nands: Vec<NandInst>,
    pub buffers: Vec<PassThrough>,
    /// Width of every canonical net, indexed by net number.
    pub net_widths: Vec<u32>,
}

/// Flattens `chip_name` within `project` into a [`FlatNetlist`].
pub fn elaborate(project: &DlsProject, chip_name: &str) -> Result<FlatNetlist, ProfileError> {
    let chip = project
        .chips
        .get(chip_name)
        .ok_or_else(|| ProfileError::Structure {
            chip: chip_name.to_string(),
            detail: format!("no chip named '{chip_name}' in project"),
        })?;

    let mut elaborator = Elaborator {
        project,
        uf: UnionFind::default(),
        nands: Vec::new(),
    };

    // Allocate the top chip's boundary bits; these become the module ports.
    let mut boundary: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    let mut input_pins = Vec::new();
    for pin in &chip.input_pins {
        let nodes = elaborator.make_bits(chip_name, pin.bit_count)?;
        boundary.insert(pin.id, nodes.clone());
        input_pins.push((pin.name.clone(), pin.bit_count, nodes));
    }
    let mut output_pins = Vec::new();
    for pin in &chip.output_pins {
        let nodes = elaborator.make_bits(chip_name, pin.bit_count)?;
        boundary.insert(pin.id, nodes.clone());
        output_pins.push((pin.name.clone(), pin.bit_count, nodes));
    }

    let mut stack = vec![chip_name.to_string()];
    elaborator.elab_chip(chip, chip_name, &boundary, &mut stack)?;

    Ok(assign_nets(&mut elaborator, &input_pins, &output_pins))
}

type PinBinding = (String, u32, Vec<usize>);

/// Groups union-find roots into canonical nets.
///
/// Each boundary pin becomes one net of its own width so the module keeps a
/// real bus interface. Every other root becomes a one-bit internal net. An
/// output-pin bit whose root already has a home elsewhere gets a pass-through
/// rather than a second name for the same signal.
fn assign_nets(
    elaborator: &mut Elaborator<'_>,
    input_pins: &[PinBinding],
    output_pins: &[PinBinding],
) -> FlatNetlist {
    let mut net_widths: Vec<u32> = Vec::new();
    let mut home: BTreeMap<usize, BitRef> = BTreeMap::new();

    let mut inputs = Vec::with_capacity(input_pins.len());
    for (name, width, nodes) in input_pins {
        let net = net_widths.len();
        net_widths.push(*width);
        for (bit, node) in nodes.iter().enumerate() {
            let root = elaborator.uf.find(*node);
            home.entry(root).or_insert(BitRef {
                net,
                bit: bit as u32,
            });
        }
        inputs.push(BoundaryPin {
            name: name.clone(),
            net,
            width: *width,
        });
    }

    let mut buffers = Vec::new();
    let mut outputs = Vec::with_capacity(output_pins.len());
    for (name, width, nodes) in output_pins {
        let net = net_widths.len();
        net_widths.push(*width);
        for (bit, node) in nodes.iter().enumerate() {
            let root = elaborator.uf.find(*node);
            let target = BitRef {
                net,
                bit: bit as u32,
            };
            match home.get(&root) {
                // Already named elsewhere: copy it across rather than aliasing.
                Some(existing) => buffers.push(PassThrough {
                    from: *existing,
                    to: target,
                }),
                None => {
                    home.insert(root, target);
                }
            }
        }
        outputs.push(BoundaryPin {
            name: name.clone(),
            net,
            width: *width,
        });
    }

    let mut resolve_node = |uf: &mut UnionFind, node: usize| -> BitRef {
        let root = uf.find(node);
        *home.entry(root).or_insert_with(|| {
            let net = net_widths.len();
            net_widths.push(1);
            BitRef { net, bit: 0 }
        })
    };

    let nands = elaborator
        .nands
        .clone()
        .into_iter()
        .map(|(a, b, y)| NandInst {
            a: resolve_node(&mut elaborator.uf, a),
            b: resolve_node(&mut elaborator.uf, b),
            y: resolve_node(&mut elaborator.uf, y),
        })
        .collect();

    FlatNetlist {
        inputs,
        outputs,
        nands,
        buffers,
        net_widths,
    }
}

struct Elaborator<'a> {
    project: &'a DlsProject,
    uf: UnionFind,
    nands: Vec<(usize, usize, usize)>,
}

impl Elaborator<'_> {
    /// Allocates one union-find node per bit of a pin, least significant first.
    fn make_bits(&mut self, chip_name: &str, width: u32) -> Result<Vec<usize>, ProfileError> {
        if width == 0 {
            return Err(ProfileError::Structure {
                chip: chip_name.to_string(),
                detail: "pin has a bit count of zero".into(),
            });
        }
        if self.uf.len() + width as usize > MAX_NODES {
            return Err(ProfileError::Limit {
                chip: chip_name.to_string(),
                detail: format!(
                    "flattening exceeds {MAX_NODES} signal bits; the chip hierarchy expands too far to compile"
                ),
            });
        }
        Ok((0..width).map(|_| self.uf.make()).collect())
    }

    fn elab_chip(
        &mut self,
        chip: &ChipDef,
        chip_name: &str,
        boundary: &BTreeMap<i64, Vec<usize>>,
        stack: &mut Vec<String>,
    ) -> Result<(), ProfileError> {
        if stack.len() > MAX_DEPTH {
            return Err(ProfileError::Structure {
                chip: chip_name.to_string(),
                detail: format!("chip nesting exceeds depth limit {MAX_DEPTH}"),
            });
        }
        check_ids_unique(chip, chip_name)?;

        // Bit nodes for pins owned by this chip's sub-chips.
        let mut sub_nodes: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
        for sub in &chip.sub_chips {
            match Builtin::parse(&sub.name) {
                Some(builtin) => {
                    self.elab_builtin(builtin, sub.id, chip_name, &mut sub_nodes)?;
                }
                None => {
                    let Some(child) = self.project.chips.get(&sub.name) else {
                        return Err(ProfileError::Unsupported {
                            chip: chip_name.to_string(),
                            detail: format!(
                                "sub-chip '{}' is a built-in or unknown chip outside the supported subset (supported: NAND, bus split/merge, BUS-N, and project chips)",
                                sub.name
                            ),
                        });
                    };
                    if stack.iter().any(|name| name == &sub.name) {
                        return Err(ProfileError::Structure {
                            chip: chip_name.to_string(),
                            detail: format!("chip reference cycle through '{}'", sub.name),
                        });
                    }
                    let mut child_boundary = BTreeMap::new();
                    for pin in child.input_pins.iter().chain(child.output_pins.iter()) {
                        let nodes = self.make_bits(&sub.name, pin.bit_count)?;
                        sub_nodes.insert((sub.id, pin.id), nodes.clone());
                        child_boundary.insert(pin.id, nodes);
                    }
                    stack.push(sub.name.clone());
                    self.elab_chip(child, &sub.name, &child_boundary, stack)?;
                    stack.pop();
                }
            }
        }

        for wire in &chip.wires {
            let source = resolve(chip_name, boundary, &sub_nodes, wire.source)?;
            let target = resolve(chip_name, boundary, &sub_nodes, wire.target)?;
            if source.len() != target.len() {
                return Err(ProfileError::Unsupported {
                    chip: chip_name.to_string(),
                    detail: format!(
                        "wire joins a {}-bit pin (owner {}, pin {}) to a {}-bit pin (owner {}, pin {})",
                        source.len(),
                        wire.source.pin_owner_id,
                        wire.source.pin_id,
                        target.len(),
                        wire.target.pin_owner_id,
                        wire.target.pin_id
                    ),
                });
            }
            for (left, right) in source.iter().zip(target.iter()) {
                self.uf.union(*left, *right);
            }
        }
        Ok(())
    }

    /// Binds a built-in's pins to bit nodes.
    ///
    /// Splitters, mergers and buses produce no logic: their narrow pins are
    /// bound to the very nodes of the wide pin they name, so the conversion is
    /// pure aliasing and costs nothing in the emitted Verilog.
    fn elab_builtin(
        &mut self,
        builtin: Builtin,
        instance_id: i64,
        chip_name: &str,
        sub_nodes: &mut BTreeMap<(i64, i64), Vec<usize>>,
    ) -> Result<(), ProfileError> {
        match builtin {
            Builtin::Nand => {
                if self.nands.len() >= MAX_INSTANCES {
                    return Err(ProfileError::Limit {
                        chip: chip_name.to_string(),
                        detail: format!(
                            "flattening exceeds {MAX_INSTANCES} NAND instances; the chip hierarchy expands too far to compile"
                        ),
                    });
                }
                let a = self.uf.make();
                let b = self.uf.make();
                let y = self.uf.make();
                sub_nodes.insert((instance_id, NAND_IN_A), vec![a]);
                sub_nodes.insert((instance_id, NAND_IN_B), vec![b]);
                sub_nodes.insert((instance_id, NAND_OUT), vec![y]);
                self.nands.push((a, b, y));
            }
            Builtin::Split { wide, piece } => {
                let bits = self.make_bits(chip_name, wide)?;
                sub_nodes.insert((instance_id, 0), bits.clone());
                for index in 0..Builtin::piece_count(wide, piece) {
                    let (start, end) = piece_bits(wide, piece, index);
                    sub_nodes.insert(
                        (instance_id, i64::from(index) + 1),
                        bits[start as usize..end as usize].to_vec(),
                    );
                }
            }
            Builtin::Merge { wide, piece } => {
                let bits = self.make_bits(chip_name, wide)?;
                let count = Builtin::piece_count(wide, piece);
                for index in 0..count {
                    let (start, end) = piece_bits(wide, piece, index);
                    sub_nodes.insert(
                        (instance_id, i64::from(index)),
                        bits[start as usize..end as usize].to_vec(),
                    );
                }
                sub_nodes.insert((instance_id, i64::from(count)), bits);
            }
            Builtin::Bus { width } => {
                // Both pins name one signal, so they share nodes outright.
                let bits = self.make_bits(chip_name, width)?;
                sub_nodes.insert((instance_id, 0), bits.clone());
                sub_nodes.insert((instance_id, 1), bits);
            }
            Builtin::BusTerminus { width } => {
                let bits = self.make_bits(chip_name, width)?;
                sub_nodes.insert((instance_id, 0), bits);
            }
        }
        Ok(())
    }
}

fn resolve(
    chip_name: &str,
    boundary: &BTreeMap<i64, Vec<usize>>,
    sub_nodes: &BTreeMap<(i64, i64), Vec<usize>>,
    addr: PinAddress,
) -> Result<Vec<usize>, ProfileError> {
    if let Some(nodes) = boundary.get(&addr.pin_owner_id) {
        return Ok(nodes.clone());
    }
    if let Some(nodes) = sub_nodes.get(&(addr.pin_owner_id, addr.pin_id)) {
        return Ok(nodes.clone());
    }
    Err(ProfileError::Structure {
        chip: chip_name.to_string(),
        detail: format!(
            "wire references unknown pin (owner {}, pin {})",
            addr.pin_owner_id, addr.pin_id
        ),
    })
}

/// Rejects a chip whose boundary-pin ids and sub-chip instance ids are not all
/// distinct.
///
/// `resolve` looks an endpoint's owner up as a boundary pin before treating it
/// as a sub-chip pin, so an id used for both silently resolves every wire on
/// that sub-chip to the boundary pin instead — mis-wiring the circuit. Two
/// sub-chips sharing an instance id would likewise overwrite each other's
/// endpoints. Both are rejected here rather than mis-converted.
fn check_ids_unique(chip: &ChipDef, chip_name: &str) -> Result<(), ProfileError> {
    let mut owners: BTreeMap<i64, &'static str> = BTreeMap::new();
    for pin in chip.input_pins.iter().chain(chip.output_pins.iter()) {
        if let Some(previous) = owners.insert(pin.id, "boundary pin") {
            return Err(ProfileError::Structure {
                chip: chip_name.to_string(),
                detail: format!("id {} is used by more than one {previous}", pin.id),
            });
        }
    }
    for sub in &chip.sub_chips {
        if let Some(previous) = owners.insert(sub.id, "sub-chip") {
            return Err(ProfileError::Structure {
                chip: chip_name.to_string(),
                detail: format!(
                    "sub-chip instance id {} collides with an existing {previous} id",
                    sub.id
                ),
            });
        }
    }
    Ok(())
}

/// Kept for callers that need the old single-bit guarantee; unused internally
/// now that widths are supported.
#[allow(dead_code)]
fn check_single_bit(chip: &ChipDef, chip_name: &str) -> Result<(), ProfileError> {
    let offending = chip
        .input_pins
        .iter()
        .chain(chip.output_pins.iter())
        .find(|pin: &&PinDef| pin.bit_count != 1);
    if let Some(pin) = offending {
        return Err(ProfileError::Unsupported {
            chip: chip_name.to_string(),
            detail: format!(
                "pin '{}' is {} bits wide; only single-bit pins are supported",
                pin.name, pin.bit_count
            ),
        });
    }
    Ok(())
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

    fn len(&self) -> usize {
        self.parent.len()
    }

    fn find(&mut self, mut node: usize) -> usize {
        while self.parent[node] != node {
            self.parent[node] = self.parent[self.parent[node]];
            node = self.parent[node];
        }
        node
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[rb] = ra;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dls::model::load_project;
    use std::path::PathBuf;

    fn project() -> DlsProject {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dls/test");
        load_project(&dir).expect("load")
    }

    #[test]
    fn and_flattens_to_two_nands() {
        let flat = elaborate(&project(), "AND").expect("elaborate AND");
        assert_eq!(flat.nands.len(), 2, "AND is two NANDs");
        assert_eq!(flat.inputs.len(), 2);
        assert_eq!(flat.outputs.len(), 1);
        // Two inputs, N1 internal output, N2 output => 4 distinct nets.
        assert_eq!(flat.net_widths.len(), 4);
        assert!(flat.net_widths.iter().all(|width| *width == 1));

        let n1 = flat.nands[0];
        let n2 = flat.nands[1];
        // N1 consumes the two module inputs.
        let input_bits: std::collections::BTreeSet<BitRef> = flat
            .inputs
            .iter()
            .map(|pin| BitRef {
                net: pin.net,
                bit: 0,
            })
            .collect();
        assert_eq!(
            [n1.a, n1.b]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
            input_bits
        );
        // N2 is fed twice by N1's output.
        assert_eq!(n1.y, n2.a);
        assert_eq!(n2.a, n2.b);
        // N2 drives the module output.
        assert_eq!(
            n2.y,
            BitRef {
                net: flat.outputs[0].net,
                bit: 0
            }
        );
    }

    #[test]
    fn one_bit_adder_elaborates() {
        let flat = elaborate(&project(), "1-bit adder").expect("elaborate adder");
        assert_eq!(flat.inputs.len(), 3, "A, B, cin");
        assert_eq!(flat.outputs.len(), 2, "carry, out");
        assert!(!flat.nands.is_empty());
    }

    fn pin(name: &str, id: i64) -> PinDef {
        wide_pin(name, id, 1)
    }

    fn wide_pin(name: &str, id: i64, bit_count: u32) -> PinDef {
        PinDef {
            name: name.into(),
            id,
            bit_count,
        }
    }

    fn wire(source: (i64, i64), target: (i64, i64)) -> crate::dls::model::Wire {
        crate::dls::model::Wire {
            source: PinAddress {
                pin_id: source.1,
                pin_owner_id: source.0,
            },
            target: PinAddress {
                pin_id: target.1,
                pin_owner_id: target.0,
            },
        }
    }

    fn single(name: &str, chip: ChipDef) -> DlsProject {
        DlsProject {
            name: "p".into(),
            chip_names: vec![name.to_string()],
            chips: BTreeMap::from([(name.to_string(), chip)]),
        }
    }

    #[test]
    fn a_split_then_merge_round_trips_a_bus_with_no_logic() {
        // 4-bit in -> 4-1BIT -> 1-4BIT -> 4-bit out, wired straight across.
        // Every narrow pin aliases a bit of the wide one, so this reduces to
        // four pass-throughs and no gates at all.
        let mut wires = Vec::new();
        wires.push(wire((1, 0), (10, 0))); // input pin -> split wide side
        for index in 1..=4_i64 {
            // split output `index` -> merge input `index - 1`, same bit.
            wires.push(wire((10, index), (11, index - 1)));
        }
        wires.push(wire((11, 4), (2, 0))); // merge wide side -> output pin

        let chip = ChipDef {
            name: "bus".into(),
            input_pins: vec![wide_pin("a", 1, 4)],
            output_pins: vec![wide_pin("y", 2, 4)],
            sub_chips: vec![
                crate::dls::model::SubChip {
                    name: "4-1BIT".into(),
                    id: 10,
                },
                crate::dls::model::SubChip {
                    name: "1-4BIT".into(),
                    id: 11,
                },
            ],
            wires,
        };

        let flat = elaborate(&single("bus", chip), "bus").expect("elaborate");
        assert!(flat.nands.is_empty(), "a bus round trip needs no gates");
        assert_eq!(flat.inputs[0].width, 4);
        assert_eq!(flat.outputs[0].width, 4);
        // Each output bit copies the same-numbered input bit.
        assert_eq!(flat.buffers.len(), 4);
        for buffer in &flat.buffers {
            assert_eq!(buffer.from.net, flat.inputs[0].net);
            assert_eq!(buffer.to.net, flat.outputs[0].net);
            assert_eq!(
                buffer.from.bit, buffer.to.bit,
                "split/merge ordering must round trip"
            );
        }
    }

    #[test]
    fn a_bus_alias_joins_its_two_pins() {
        let chip = ChipDef {
            name: "route".into(),
            input_pins: vec![pin("a", 1)],
            output_pins: vec![pin("y", 2)],
            sub_chips: vec![crate::dls::model::SubChip {
                name: "BUS-1".into(),
                id: 10,
            }],
            wires: vec![wire((1, 0), (10, 0)), wire((10, 1), (2, 0))],
        };
        let flat = elaborate(&single("route", chip), "route").expect("elaborate");
        assert!(flat.nands.is_empty());
        assert_eq!(
            flat.buffers.len(),
            1,
            "input reaches output through the bus"
        );
    }

    #[test]
    fn a_wire_between_pins_of_different_widths_is_rejected() {
        let chip = ChipDef {
            name: "mixed".into(),
            input_pins: vec![wide_pin("a", 1, 8)],
            output_pins: vec![pin("y", 2)],
            sub_chips: Vec::new(),
            wires: vec![wire((1, 0), (2, 0))],
        };
        let error = elaborate(&single("mixed", chip), "mixed").unwrap_err();
        match error {
            ProfileError::Unsupported { detail, .. } => {
                assert!(detail.contains("8-bit"), "{detail}");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn an_unsupported_builtin_still_names_itself() {
        let chip = ChipDef {
            name: "clocky".into(),
            input_pins: Vec::new(),
            output_pins: vec![pin("q", 1)],
            sub_chips: vec![crate::dls::model::SubChip {
                name: "CLOCK".into(),
                id: 2,
            }],
            wires: vec![wire((2, 0), (1, 0))],
        };
        let error = elaborate(&single("clocky", chip), "clocky").unwrap_err();
        match error {
            ProfileError::Unsupported { detail, .. } => {
                assert!(detail.contains("CLOCK"), "{detail}")
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn rejects_sub_chip_id_colliding_with_a_boundary_pin_id() {
        // Regression: `resolve` matched boundary pins first, so a sub-chip whose
        // instance id equalled a pin id had all of its wires silently attached
        // to that pin — surfacing later as misleading NET_NO_DRIVER errors.
        let chip = ChipDef {
            name: "C".into(),
            input_pins: vec![pin("a", 100)],
            output_pins: vec![pin("y", 200)],
            // Instance id 100 collides with input pin id 100.
            sub_chips: vec![crate::dls::model::SubChip {
                name: "NAND".into(),
                id: 100,
            }],
            wires: vec![
                wire((100, 0), (100, 0)),
                wire((100, 0), (100, 1)),
                wire((100, 2), (200, 0)),
            ],
        };

        let error = elaborate(&single("C", chip), "C").unwrap_err();
        match error {
            ProfileError::Structure { chip, detail } => {
                assert_eq!(chip, "C");
                assert!(detail.contains("collides"), "{detail}");
            }
            other => panic!("expected Structure, got {other:?}"),
        }
    }

    #[test]
    fn rejects_hierarchies_that_expand_past_the_instance_cap() {
        // Regression: each level instantiating its child twice doubles the
        // instance count, so a depth-20 chain built 2^20 NANDs (~2 GB) before
        // the kernel's own limits could reject it.
        const DEPTH: i64 = 20;
        let mut chips = BTreeMap::new();
        chips.insert(
            "C0".to_string(),
            ChipDef {
                name: "C0".into(),
                input_pins: vec![pin("a", 1), pin("b", 2)],
                output_pins: vec![pin("y", 3)],
                sub_chips: vec![crate::dls::model::SubChip {
                    name: "NAND".into(),
                    id: 4,
                }],
                wires: vec![
                    wire((1, 0), (4, 0)),
                    wire((2, 0), (4, 1)),
                    wire((4, 2), (3, 0)),
                ],
            },
        );
        for level in 1..=DEPTH {
            let child = format!("C{}", level - 1);
            chips.insert(
                format!("C{level}"),
                ChipDef {
                    name: format!("C{level}"),
                    input_pins: vec![pin("a", 1), pin("b", 2)],
                    output_pins: vec![pin("y", 3)],
                    sub_chips: vec![
                        crate::dls::model::SubChip {
                            name: child.clone(),
                            id: 10,
                        },
                        crate::dls::model::SubChip {
                            name: child,
                            id: 11,
                        },
                    ],
                    wires: vec![
                        wire((1, 0), (10, 1)),
                        wire((2, 0), (10, 2)),
                        wire((10, 3), (11, 1)),
                        wire((2, 0), (11, 2)),
                        wire((11, 3), (3, 0)),
                    ],
                },
            );
        }
        let project = DlsProject {
            name: "blowup".into(),
            chip_names: vec![format!("C{DEPTH}")],
            chips,
        };

        let error = elaborate(&project, &format!("C{DEPTH}")).unwrap_err();
        match error {
            ProfileError::Limit { detail, .. } => {
                assert!(
                    detail.contains("NAND instances") || detail.contains("signal bits"),
                    "{detail}"
                );
            }
            other => panic!("expected Limit, got {other:?}"),
        }
    }

    #[test]
    fn unknown_chip_is_a_structure_error() {
        let error = elaborate(&project(), "does-not-exist").unwrap_err();
        assert!(matches!(error, ProfileError::Structure { .. }));
    }
}
