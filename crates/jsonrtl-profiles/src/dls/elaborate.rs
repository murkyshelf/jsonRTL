//! Flattens a hierarchical DLS chip into a flat netlist of NAND primitives.
//!
//! DLS chips compose other chips down to the built-in NAND (input pins `0`,`1`;
//! output pin `2`). Elaboration inlines every custom sub-chip recursively and
//! uses union-find over wire endpoints to coalesce connected pins into nets.

use std::collections::BTreeMap;

use crate::{
    ProfileError,
    dls::model::{ChipDef, DlsProject, PinAddress, PinDef},
};

/// Guards against pathological or cyclic chip nesting.
const MAX_DEPTH: usize = 256;

/// The built-in NAND primitive's fixed pin ids.
const NAND_IN_A: i64 = 0;
const NAND_IN_B: i64 = 1;
const NAND_OUT: i64 = 2;

/// A module-boundary pin bound to a flat net index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryPin {
    pub name: String,
    pub net: usize,
}

/// A NAND instance wired to flat net indices (`Y = ~(A & B)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NandInst {
    pub a: usize,
    pub b: usize,
    pub y: usize,
}

/// A chip flattened to NAND primitives over a dense range of net indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatNetlist {
    pub inputs: Vec<BoundaryPin>,
    pub outputs: Vec<BoundaryPin>,
    pub nands: Vec<NandInst>,
    pub net_count: usize,
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
    check_single_bit(chip, chip_name)?;

    let mut elaborator = Elaborator {
        project,
        uf: UnionFind::default(),
        nands: Vec::new(),
    };

    // Allocate the top chip's boundary nodes; these become the module ports.
    let mut boundary = BTreeMap::new();
    let mut input_nodes = Vec::new();
    for pin in &chip.input_pins {
        let node = elaborator.uf.make();
        boundary.insert(pin.id, node);
        input_nodes.push((pin.name.clone(), node));
    }
    let mut output_nodes = Vec::new();
    for pin in &chip.output_pins {
        let node = elaborator.uf.make();
        boundary.insert(pin.id, node);
        output_nodes.push((pin.name.clone(), node));
    }

    let mut stack = vec![chip_name.to_string()];
    elaborator.elab_chip(chip, chip_name, &boundary, &mut stack)?;

    // Assign dense net indices to the union-find roots that the netlist uses,
    // in a stable order (inputs, outputs, then NAND pins).
    let mut net_of_root: BTreeMap<usize, usize> = BTreeMap::new();
    let inputs = input_nodes
        .iter()
        .map(|(name, node)| BoundaryPin {
            name: name.clone(),
            net: intern(&mut elaborator.uf, &mut net_of_root, *node),
        })
        .collect();
    let outputs = output_nodes
        .iter()
        .map(|(name, node)| BoundaryPin {
            name: name.clone(),
            net: intern(&mut elaborator.uf, &mut net_of_root, *node),
        })
        .collect();
    let nands = elaborator
        .nands
        .clone()
        .into_iter()
        .map(|(a, b, y)| NandInst {
            a: intern(&mut elaborator.uf, &mut net_of_root, a),
            b: intern(&mut elaborator.uf, &mut net_of_root, b),
            y: intern(&mut elaborator.uf, &mut net_of_root, y),
        })
        .collect();

    Ok(FlatNetlist {
        inputs,
        outputs,
        nands,
        net_count: net_of_root.len(),
    })
}

struct Elaborator<'a> {
    project: &'a DlsProject,
    uf: UnionFind,
    nands: Vec<(usize, usize, usize)>,
}

impl Elaborator<'_> {
    fn elab_chip(
        &mut self,
        chip: &ChipDef,
        chip_name: &str,
        boundary: &BTreeMap<i64, usize>,
        stack: &mut Vec<String>,
    ) -> Result<(), ProfileError> {
        if stack.len() > MAX_DEPTH {
            return Err(ProfileError::Structure {
                chip: chip_name.to_string(),
                detail: format!("chip nesting exceeds depth limit {MAX_DEPTH}"),
            });
        }

        // Endpoint nodes for pins owned by this chip's sub-chips.
        let mut sub_nodes: BTreeMap<(i64, i64), usize> = BTreeMap::new();
        let project = self.project;
        for sub in &chip.sub_chips {
            if sub.name == "NAND" {
                let a = self.uf.make();
                let b = self.uf.make();
                let y = self.uf.make();
                sub_nodes.insert((sub.id, NAND_IN_A), a);
                sub_nodes.insert((sub.id, NAND_IN_B), b);
                sub_nodes.insert((sub.id, NAND_OUT), y);
                self.nands.push((a, b, y));
            } else if let Some(child) = project.chips.get(&sub.name) {
                if stack.iter().any(|name| name == &sub.name) {
                    return Err(ProfileError::Structure {
                        chip: chip_name.to_string(),
                        detail: format!("chip reference cycle through '{}'", sub.name),
                    });
                }
                check_single_bit(child, &sub.name)?;
                let mut child_boundary = BTreeMap::new();
                for pin in child.input_pins.iter().chain(child.output_pins.iter()) {
                    let node = self.uf.make();
                    sub_nodes.insert((sub.id, pin.id), node);
                    child_boundary.insert(pin.id, node);
                }
                stack.push(sub.name.clone());
                self.elab_chip(child, &sub.name, &child_boundary, stack)?;
                stack.pop();
            } else {
                return Err(ProfileError::Unsupported {
                    chip: chip_name.to_string(),
                    detail: format!(
                        "sub-chip '{}' is a built-in or unknown chip outside the supported subset (only NAND and project chips are supported)",
                        sub.name
                    ),
                });
            }
        }

        for wire in &chip.wires {
            let source = resolve(chip_name, boundary, &sub_nodes, wire.source)?;
            let target = resolve(chip_name, boundary, &sub_nodes, wire.target)?;
            self.uf.union(source, target);
        }
        Ok(())
    }
}

fn resolve(
    chip_name: &str,
    boundary: &BTreeMap<i64, usize>,
    sub_nodes: &BTreeMap<(i64, i64), usize>,
    addr: PinAddress,
) -> Result<usize, ProfileError> {
    if let Some(&node) = boundary.get(&addr.pin_owner_id) {
        return Ok(node);
    }
    if let Some(&node) = sub_nodes.get(&(addr.pin_owner_id, addr.pin_id)) {
        return Ok(node);
    }
    Err(ProfileError::Structure {
        chip: chip_name.to_string(),
        detail: format!(
            "wire references unknown pin (owner {}, pin {})",
            addr.pin_owner_id, addr.pin_id
        ),
    })
}

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

fn intern(uf: &mut UnionFind, net_of_root: &mut BTreeMap<usize, usize>, node: usize) -> usize {
    let root = uf.find(node);
    let next = net_of_root.len();
    *net_of_root.entry(root).or_insert(next)
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
        assert_eq!(flat.net_count, 4);

        let n1 = flat.nands[0];
        let n2 = flat.nands[1];
        // N1 consumes the two module inputs.
        let input_nets: std::collections::BTreeSet<usize> =
            flat.inputs.iter().map(|pin| pin.net).collect();
        assert_eq!(
            [n1.a, n1.b]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
            input_nets
        );
        // N2 is fed twice by N1's output.
        assert_eq!(n1.y, n2.a);
        assert_eq!(n2.a, n2.b);
        // N2 drives the module output.
        assert_eq!(n2.y, flat.outputs[0].net);
    }

    #[test]
    fn one_bit_adder_elaborates() {
        let flat = elaborate(&project(), "1-bit adder").expect("elaborate adder");
        assert_eq!(flat.inputs.len(), 3, "A, B, cin");
        assert_eq!(flat.outputs.len(), 2, "carry, out");
        assert!(!flat.nands.is_empty());
    }

    #[test]
    fn unknown_chip_is_a_structure_error() {
        let error = elaborate(&project(), "does-not-exist").unwrap_err();
        assert!(matches!(error, ProfileError::Structure { .. }));
    }
}
