//! The DLS built-in chips this profile understands, and their pin layouts.
//!
//! Everything here was decoded from real project files; see
//! `docs/superpowers/specs/2026-07-25-multi-bit-buses-design.md` for the
//! evidence, in particular the carry chain that fixes the bit ordering.

/// A recognised DLS built-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    /// The one true combinational primitive: inputs `0`,`1`, output `2`.
    Nand,
    /// `X-YBIT` with `X > Y`: one wide input on pin `0`, `X / Y` narrow
    /// outputs on pins `1..=count`, most significant first.
    Split { wide: u32, piece: u32 },
    /// `X-YBIT` with `X < Y`: `Y / X` narrow inputs on pins `0..count`, most
    /// significant first, and one wide output on pin `count`.
    Merge { wide: u32, piece: u32 },
    /// `BUS-N`: a fan-out alias. Pin `0` in, pin `1` out, same signal.
    Bus { width: u32 },
    /// `BUS-TERMINUS-N`: the far end of a drawn bus line. Pin `0` in, no
    /// output, drives nothing.
    BusTerminus { width: u32 },
}

impl Builtin {
    /// Classifies a DLS sub-chip name, or `None` if it is not a built-in this
    /// profile supports.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        if name == "NAND" {
            return Some(Self::Nand);
        }
        if let Some(width) = name.strip_prefix("BUS-TERMINUS-") {
            return width.parse().ok().map(|width| Self::BusTerminus { width });
        }
        if let Some(width) = name.strip_prefix("BUS-") {
            return width.parse().ok().map(|width| Self::Bus { width });
        }
        Self::parse_converter(name)
    }

    /// Parses `X-YBIT`, the split/merge family.
    fn parse_converter(name: &str) -> Option<Self> {
        let body = name.strip_suffix("BIT")?;
        let (from, to) = body.split_once('-')?;
        let from: u32 = from.parse().ok()?;
        let to: u32 = to.parse().ok()?;
        if from == 0 || to == 0 || from == to {
            return None;
        }
        let (wide, piece) = if from > to { (from, to) } else { (to, from) };
        // A converter only makes sense when the wide side divides evenly.
        if wide % piece != 0 {
            return None;
        }
        Some(if from > to {
            Self::Split { wide, piece }
        } else {
            Self::Merge { wide, piece }
        })
    }

    /// The number of narrow pins on a split or merge.
    #[must_use]
    pub const fn piece_count(wide: u32, piece: u32) -> u32 {
        wide / piece
    }
}

/// The bit range `[start, end)` of the wide side covered by narrow piece
/// `index`, counting from the most significant piece at index `0`.
///
/// DLS orders both splits and merges most significant first, so piece `0` is
/// the top of the bus.
#[must_use]
pub const fn piece_bits(wide: u32, piece: u32, index: u32) -> (u32, u32) {
    let end = wide - index * piece;
    (end - piece, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_converter_family() {
        assert_eq!(
            Builtin::parse("8-1BIT"),
            Some(Builtin::Split { wide: 8, piece: 1 })
        );
        assert_eq!(
            Builtin::parse("1-8BIT"),
            Some(Builtin::Merge { wide: 8, piece: 1 })
        );
        assert_eq!(
            Builtin::parse("8-4BIT"),
            Some(Builtin::Split { wide: 8, piece: 4 })
        );
        assert_eq!(
            Builtin::parse("4-8BIT"),
            Some(Builtin::Merge { wide: 8, piece: 4 })
        );
        assert_eq!(
            Builtin::parse("4-1BIT"),
            Some(Builtin::Split { wide: 4, piece: 1 })
        );
    }

    #[test]
    fn recognises_bus_routing_and_nand() {
        assert_eq!(Builtin::parse("NAND"), Some(Builtin::Nand));
        assert_eq!(Builtin::parse("BUS-8"), Some(Builtin::Bus { width: 8 }));
        assert_eq!(
            Builtin::parse("BUS-TERMINUS-8"),
            Some(Builtin::BusTerminus { width: 8 })
        );
        assert_eq!(Builtin::parse("BUS-1"), Some(Builtin::Bus { width: 1 }));
    }

    #[test]
    fn rejects_names_outside_the_supported_subset() {
        for name in [
            "CLOCK",
            "PULSE",
            "3-STATE BUFFER",
            "7-SEGMENT",
            "ROM",
            "3-5BIT", // not an even division
            "0-8BIT",
            "AND",
        ] {
            assert_eq!(Builtin::parse(name), None, "accepted {name}");
        }
    }

    #[test]
    fn pieces_run_most_significant_first() {
        // 8-1BIT: output pin 1 is the MSB, pin 8 the LSB. Pin j is index j-1.
        assert_eq!(piece_bits(8, 1, 0), (7, 8));
        assert_eq!(piece_bits(8, 1, 7), (0, 1));
        // 8-4BIT: pin 1 is the high nibble, pin 2 the low one.
        assert_eq!(piece_bits(8, 4, 0), (4, 8));
        assert_eq!(piece_bits(8, 4, 1), (0, 4));
    }
}
