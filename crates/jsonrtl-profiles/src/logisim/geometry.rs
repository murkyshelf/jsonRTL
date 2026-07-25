//! Where each component's ports sit on the Logisim sheet.
//!
//! Logisim connectivity is geometric: a wire attaches to whatever port shares
//! its endpoint. Reconstructing a netlist therefore means recomputing port
//! positions from each component's anchor, facing, size, and input count.
//!
//! **This module is the one part of the Logisim profile that cannot be verified
//! without real `.circ` files**, so every rule below is stated as an explicit,
//! testable assumption. Elaboration treats any wire endpoint that fails to land
//! on a port as an error rather than dropping it, so a wrong rule here surfaces
//! as a clear diagnostic instead of silently mis-wired Verilog.

use crate::logisim::model::{Comp, Point};

/// The logical role a port plays on its component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortRole {
    /// Nth input, counted from the top/left as drawn.
    Input(usize),
    Output,
}

/// A port and the sheet position it occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortPoint {
    pub role: PortRole,
    pub point: Point,
}

/// Which way a component faces. Logisim defaults to east.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facing {
    East,
    West,
    North,
    South,
}

impl Facing {
    /// Reads the `facing` attribute, defaulting to east.
    #[must_use]
    pub fn of(comp: &Comp) -> Self {
        match comp.attr("facing") {
            Some("west") => Self::West,
            Some("north") => Self::North,
            Some("south") => Self::South,
            _ => Self::East,
        }
    }

    /// Moves `origin` back along the facing direction by `distance`, i.e.
    /// towards the component's inputs.
    #[must_use]
    fn behind(self, origin: Point, distance: i64) -> Point {
        match self {
            Self::East => Point {
                x: origin.x - distance,
                y: origin.y,
            },
            Self::West => Point {
                x: origin.x + distance,
                y: origin.y,
            },
            Self::South => Point {
                x: origin.x,
                y: origin.y - distance,
            },
            Self::North => Point {
                x: origin.x,
                y: origin.y + distance,
            },
        }
    }

    /// Spreads a point sideways relative to the facing direction.
    #[must_use]
    fn across(self, origin: Point, offset: i64) -> Point {
        match self {
            Self::East | Self::West => Point {
                x: origin.x,
                y: origin.y + offset,
            },
            Self::North | Self::South => Point {
                x: origin.x + offset,
                y: origin.y,
            },
        }
    }
}

/// Vertical offsets of a gate's input ports relative to its axis.
///
/// Assumption: inputs step 10 units apart and are centred on the axis; when the
/// count is even the centre slot is skipped so every port still lands on the
/// 10-unit grid Logisim snaps to. That gives `-10,+10` for two inputs,
/// `-10,0,+10` for three, and `-20,-10,+10,+20` for four.
///
/// This is the assumption most likely to need correcting against a real file:
/// if it is wrong, wire endpoints stop matching ports and elaboration reports
/// the exact coordinate that failed to resolve.
#[must_use]
pub fn input_offsets(inputs: usize) -> Vec<i64> {
    (0..inputs)
        .map(|index| {
            // Offset in half-steps of 10 so even counts straddle the axis.
            let half_steps = 2 * index as i64 - (inputs as i64 - 1);
            let raw = half_steps * 5;
            if half_steps % 2 == 0 {
                raw
            } else {
                // Round away from the axis onto the grid.
                raw + raw.signum() * 5
            }
        })
        .collect()
}

/// The distance from a gate's output anchor back to its input edge.
///
/// Assumption: the `size` attribute is that distance, defaulting to 30 (50 and
/// 70 are the other values Logisim offers). A NOT gate defaults to 30 as well.
#[must_use]
pub fn gate_depth(comp: &Comp) -> i64 {
    comp.attr_int("size").unwrap_or(30)
}

/// Port positions for a gate with `inputs` inputs.
///
/// Assumption: a gate's `loc` is its **output** anchor, and inputs lie on the
/// opposite edge, `gate_depth` away, spread by [`input_offsets`].
#[must_use]
pub fn gate_ports(comp: &Comp, inputs: usize) -> Vec<PortPoint> {
    let facing = Facing::of(comp);
    let depth = gate_depth(comp);
    let input_edge = facing.behind(comp.loc, depth);

    let mut ports = Vec::with_capacity(inputs + 1);
    ports.push(PortPoint {
        role: PortRole::Output,
        point: comp.loc,
    });
    for (index, offset) in input_offsets(inputs).into_iter().enumerate() {
        ports.push(PortPoint {
            role: PortRole::Input(index),
            point: facing.across(input_edge, offset),
        });
    }
    ports
}

/// Port position for a single-input component (NOT, buffer).
///
/// Assumption: the input sits directly behind the output on the axis.
#[must_use]
pub fn unary_ports(comp: &Comp) -> Vec<PortPoint> {
    let facing = Facing::of(comp);
    vec![
        PortPoint {
            role: PortRole::Output,
            point: comp.loc,
        },
        PortPoint {
            role: PortRole::Input(0),
            point: facing.behind(comp.loc, gate_depth(comp)),
        },
    ]
}

/// Port position for a Pin, whose anchor *is* its connection point.
#[must_use]
pub fn pin_port(comp: &Comp, is_output_pin: bool) -> PortPoint {
    // A Pin that drives the sheet is an input to the enclosing module, and a
    // Pin that reads the sheet is an output of it — so the roles invert.
    PortPoint {
        role: if is_output_pin {
            PortRole::Input(0)
        } else {
            PortRole::Output
        },
        point: comp.loc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn comp(loc: (i64, i64), attrs: &[(&str, &str)]) -> Comp {
        Comp {
            lib: Some("1".into()),
            name: "AND Gate".into(),
            loc: Point { x: loc.0, y: loc.1 },
            attrs: attrs
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn input_offsets_straddle_the_axis_and_stay_on_grid() {
        assert_eq!(input_offsets(1), vec![0]);
        assert_eq!(input_offsets(2), vec![-10, 10]);
        assert_eq!(input_offsets(3), vec![-10, 0, 10]);
        // Even counts skip the centre slot so ports stay on the grid.
        assert_eq!(input_offsets(4), vec![-20, -10, 10, 20]);
        assert_eq!(input_offsets(5), vec![-20, -10, 0, 10, 20]);
        // Every offset must land on the 10-unit grid Logisim snaps to.
        for count in 1..=8 {
            for offset in input_offsets(count) {
                assert_eq!(offset % 10, 0, "count {count} produced {offset}");
            }
        }
    }

    #[test]
    fn east_facing_gate_puts_output_at_the_anchor() {
        let gate = comp((200, 100), &[("inputs", "2")]);
        let ports = gate_ports(&gate, 2);
        assert_eq!(
            ports[0],
            PortPoint {
                role: PortRole::Output,
                point: Point { x: 200, y: 100 }
            }
        );
        // Inputs sit one depth west, straddling the axis.
        assert_eq!(ports[1].point, Point { x: 170, y: 90 });
        assert_eq!(ports[2].point, Point { x: 170, y: 110 });
    }

    #[test]
    fn facing_rotates_the_input_edge() {
        let west = comp((200, 100), &[("inputs", "2"), ("facing", "west")]);
        let ports = gate_ports(&west, 2);
        assert_eq!(ports[1].point, Point { x: 230, y: 90 });

        let south = comp((200, 100), &[("inputs", "2"), ("facing", "south")]);
        let ports = gate_ports(&south, 2);
        assert_eq!(ports[1].point, Point { x: 190, y: 70 });
    }

    #[test]
    fn size_attribute_sets_the_input_distance() {
        let wide = comp((200, 100), &[("inputs", "2"), ("size", "50")]);
        assert_eq!(gate_depth(&wide), 50);
        assert_eq!(gate_ports(&wide, 2)[1].point.x, 150);
    }

    #[test]
    fn a_driving_pin_is_a_module_input() {
        let pin = comp((100, 100), &[]);
        assert_eq!(pin_port(&pin, false).role, PortRole::Output);
        assert_eq!(pin_port(&pin, true).role, PortRole::Input(0));
    }
}
