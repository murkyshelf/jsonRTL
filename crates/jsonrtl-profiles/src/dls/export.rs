//! Export flattened, technology-mapped Yosys JSON as a complete DLS project.
//!
//! DLS stores sequential circuits as ordinary NAND networks with feedback.
//! These helpers follow its bundled master/slave D-latch flip-flop design.
//! The canonical kernel is deliberately not involved in this conversion.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::ProfileError;

const MAX_JSON_BYTES: usize = 32 * 1024 * 1024;
const MAX_CHIP_BYTES: usize = 8 * 1024 * 1024;
const MAX_PROJECT_BYTES: usize = 64 * 1024 * 1024;
const MAX_CELLS: usize = 10_000;
const MAX_PORT_BITS: usize = 4096;
const VERSION: &str = "2.1.6";
const CONST_ONE: &str = "JSONRTL_CONST1";
const CONST_ZERO: &str = "JSONRTL_CONST0";
const LATCH: &str = "JSONRTL_D_LATCH";

/// Complete deterministic project contents, ready to write to a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportProject {
    pub top_chip: String,
    pub project_description: String,
    pub chips: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(untagged)]
enum Bit {
    Wire(u32),
    Constant(String),
}

#[derive(Deserialize)]
struct Design {
    #[serde(deserialize_with = "unique_map")]
    modules: BTreeMap<String, Module>,
}

#[derive(Deserialize)]
struct Module {
    #[serde(default, deserialize_with = "unique_map")]
    attributes: BTreeMap<String, Value>,
    #[serde(deserialize_with = "unique_map")]
    ports: BTreeMap<String, Port>,
    #[serde(deserialize_with = "unique_map")]
    cells: BTreeMap<String, Cell>,
    #[serde(default, deserialize_with = "unique_map")]
    netnames: BTreeMap<String, Netname>,
    #[serde(default, deserialize_with = "unique_map")]
    memories: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct Port {
    direction: String,
    bits: Vec<Bit>,
    #[serde(default)]
    offset: i64,
    #[serde(default)]
    upto: u8,
}

#[derive(Deserialize)]
struct Netname {
    bits: Vec<Bit>,
    #[serde(default, deserialize_with = "unique_map")]
    attributes: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct Cell {
    #[serde(rename = "type")]
    kind: String,
    #[serde(deserialize_with = "unique_map")]
    parameters: BTreeMap<String, Value>,
    #[serde(default, deserialize_with = "unique_map")]
    attributes: BTreeMap<String, Value>,
    #[serde(deserialize_with = "unique_map")]
    port_directions: BTreeMap<String, String>,
    #[serde(deserialize_with = "unique_map")]
    connections: BTreeMap<String, Vec<Bit>>,
}

fn unique_map<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct MapVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for MapVisitor<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an object with unique keys")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, T>()? {
                if result.insert(key.clone(), value).is_some() {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate object key '{key}'"
                    )));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
}

#[derive(Clone, Copy)]
enum Logic {
    Nand,
    Not,
    Dff {
        positive: bool,
        reset: Option<(bool, bool)>,
    },
    Latch {
        positive: bool,
    },
}

impl Logic {
    fn parse(kind: &str) -> Option<Self> {
        match kind {
            "$_NAND_" => Some(Self::Nand),
            "$_NOT_" => Some(Self::Not),
            "$_DFF_P_" => Some(Self::Dff {
                positive: true,
                reset: None,
            }),
            "$_DFF_N_" => Some(Self::Dff {
                positive: false,
                reset: None,
            }),
            "$_DLATCH_P_" => Some(Self::Latch { positive: true }),
            "$_DLATCH_N_" => Some(Self::Latch { positive: false }),
            _ => {
                let mode = kind.strip_prefix("$_DFF_")?.strip_suffix('_')?;
                let chars: Vec<_> = mode.chars().collect();
                if chars.len() != 3
                    || !matches!(chars[0], 'P' | 'N')
                    || !matches!(chars[1], 'P' | 'N')
                    || !matches!(chars[2], '0' | '1')
                {
                    return None;
                }
                Some(Self::Dff {
                    positive: chars[0] == 'P',
                    reset: Some((chars[1] == 'P', chars[2] == '1')),
                })
            }
        }
    }

    fn inputs(self) -> &'static [&'static str] {
        match self {
            Self::Nand => &["A", "B"],
            Self::Not => &["A"],
            Self::Dff { reset: None, .. } => &["D", "C"],
            Self::Dff { reset: Some(_), .. } => &["D", "C", "R"],
            Self::Latch { .. } => &["D", "E"],
        }
    }

    fn output(self) -> &'static str {
        match self {
            Self::Nand | Self::Not => "Y",
            _ => "Q",
        }
    }

    fn sequential(self) -> bool {
        !matches!(self, Self::Nand | Self::Not)
    }

    fn helper(self) -> String {
        match self {
            Self::Dff { positive, reset } => {
                let mut name = format!("JSONRTL_DFF_{}", if positive { 'P' } else { 'N' });
                if let Some((active_high, value)) = reset {
                    name.push(if active_high { 'P' } else { 'N' });
                    name.push(if value { '1' } else { '0' });
                }
                name
            }
            Self::Latch { positive: true } => LATCH.into(),
            Self::Latch { positive: false } => "JSONRTL_D_LATCH_N".into(),
            _ => "NAND".into(),
        }
    }
}

fn structure(top: &str, detail: impl Into<String>) -> ProfileError {
    ProfileError::Structure {
        chip: top.into(),
        detail: detail.into(),
    }
}

fn unsupported(top: &str, detail: impl Into<String>) -> ProfileError {
    ProfileError::Unsupported {
        chip: top.into(),
        detail: detail.into(),
    }
}

fn limit(top: &str, detail: impl Into<String>) -> ProfileError {
    ProfileError::Limit {
        chip: top.into(),
        detail: detail.into(),
    }
}

fn validate_bits(top: &str, bits: &[Bit]) -> Result<(), ProfileError> {
    for bit in bits {
        match bit {
            Bit::Wire(id) if *id >= 2 => {}
            Bit::Wire(id) => {
                return Err(structure(
                    top,
                    format!("invalid Yosys signal bit {id}; constants must be strings"),
                ));
            }
            Bit::Constant(state) if state == "0" || state == "1" => {}
            Bit::Constant(state) => {
                return Err(unsupported(
                    top,
                    format!(
                        "unknown or high-impedance bit '{state}' (x/z cannot be represented faithfully)"
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_attributes(
    top: &str,
    attributes: &BTreeMap<String, Value>,
) -> Result<(), ProfileError> {
    if attributes.contains_key("init") {
        return Err(unsupported(
            top,
            "explicit init state cannot be represented faithfully by DLS NAND latches",
        ));
    }
    for name in ["blackbox", "whitebox"] {
        if let Some(value) = attributes.get(name) {
            let disabled = value == &json!(0)
                || value == &json!(false)
                || value
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.chars().all(|c| c == '0'));
            if !disabled {
                return Err(unsupported(
                    top,
                    format!("{name} module or cell cannot be exported"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_module(top: &str, module: &Module) -> Result<BTreeMap<String, Logic>, ProfileError> {
    validate_attributes(top, &module.attributes)?;
    if !module.memories.is_empty() {
        return Err(unsupported(
            top,
            "remaining memories must be synthesized into gates and registers",
        ));
    }
    if module.cells.len() > MAX_CELLS {
        return Err(limit(top, format!("more than {MAX_CELLS} cells")));
    }
    let mut drivers: BTreeMap<Bit, Option<&str>> = BTreeMap::new();
    for (name, port) in &module.ports {
        if port.bits.is_empty() {
            return Err(structure(top, format!("port '{name}' has zero width")));
        }
        if port.bits.len() > MAX_PORT_BITS {
            return Err(limit(
                top,
                format!("port '{name}' exceeds {MAX_PORT_BITS} bits"),
            ));
        }
        if port.upto > 1 || port.offset.checked_add(port.bits.len() as i64).is_none() {
            return Err(structure(
                top,
                format!("port '{name}' has invalid offset/upto"),
            ));
        }
        validate_bits(top, &port.bits)?;
        match port.direction.as_str() {
            "input" => {
                for bit in &port.bits {
                    if matches!(bit, Bit::Constant(_)) {
                        return Err(structure(
                            top,
                            format!("input '{name}' contains a constant"),
                        ));
                    }
                    if drivers.insert(bit.clone(), None).is_some() {
                        return Err(structure(
                            top,
                            format!("multiple drivers for {bit:?} at input '{name}'"),
                        ));
                    }
                }
            }
            "output" => {}
            other => {
                return Err(unsupported(
                    top,
                    format!("port '{name}' has unsupported direction '{other}' (tri-state/inout)"),
                ));
            }
        }
    }
    for net in module.netnames.values() {
        validate_attributes(top, &net.attributes)?;
        validate_bits(top, &net.bits)?;
    }
    let mut logic = BTreeMap::new();
    for (name, cell) in &module.cells {
        validate_attributes(top, &cell.attributes)?;
        let gate = Logic::parse(&cell.kind).ok_or_else(|| unsupported(top, format!("cell '{name}' uses unsupported remaining cell '{}' (requires NAND/NOT or supported DFF/DLATCH mapping)",cell.kind)))?;
        if !cell.parameters.is_empty() {
            return Err(unsupported(
                top,
                format!(
                    "cell '{name}' has unexpected parameters for '{}'",
                    cell.kind
                ),
            ));
        }
        if cell.connections.len() != gate.inputs().len() + 1
            || cell.port_directions.len() != cell.connections.len()
        {
            return Err(structure(
                top,
                format!("cell '{name}' has incorrect connection/port-direction set"),
            ));
        }
        for (pin, direction) in gate
            .inputs()
            .iter()
            .map(|p| (*p, "input"))
            .chain(std::iter::once((gate.output(), "output")))
        {
            let bits = cell
                .connections
                .get(pin)
                .ok_or_else(|| structure(top, format!("cell '{name}' is missing pin '{pin}'")))?;
            if bits.len() != 1
                || cell.port_directions.get(pin).map(String::as_str) != Some(direction)
            {
                return Err(structure(
                    top,
                    format!("cell '{name}' pin '{pin}' must be one-bit {direction}"),
                ));
            }
            validate_bits(top, bits)?;
        }
        let output = &cell.connections[gate.output()][0];
        if matches!(output, Bit::Constant(_)) {
            return Err(structure(top, format!("cell '{name}' drives a constant")));
        }
        if drivers.insert(output.clone(), Some(name)).is_some() {
            return Err(structure(
                top,
                format!("multiple drivers for {output:?} at cell '{name}'"),
            ));
        }
        logic.insert(name.clone(), gate);
    }
    let mut indegrees = BTreeMap::new();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (name, cell) in &module.cells {
        let gate = logic[name];
        let mut dependencies = BTreeSet::new();
        for pin in gate.inputs() {
            let bit = &cell.connections[*pin][0];
            if let Bit::Wire(_) = bit {
                let driver = drivers.get(bit).ok_or_else(|| {
                    structure(
                        top,
                        format!("undriven live bit {bit:?} at cell '{name}' pin '{pin}'"),
                    )
                })?;
                if !gate.sequential() {
                    if let Some(driver) = driver {
                        if !logic[*driver].sequential() {
                            dependencies.insert(*driver);
                        }
                    }
                }
            }
        }
        if !gate.sequential() {
            indegrees.insert(name.as_str(), dependencies.len());
            for dependency in dependencies {
                dependents.entry(dependency).or_default().push(name);
            }
        }
    }
    for (name, port) in &module.ports {
        if port.direction == "output" {
            for bit in &port.bits {
                if matches!(bit, Bit::Wire(_)) && !drivers.contains_key(bit) {
                    return Err(structure(
                        top,
                        format!("undriven live output bit {bit:?} at '{name}'"),
                    ));
                }
            }
        }
    }
    let mut ready: VecDeque<_> = indegrees
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(name, _)| *name)
        .collect();
    let mut visited = 0;
    while let Some(name) = ready.pop_front() {
        visited += 1;
        if let Some(children) = dependents.get(name) {
            for child in children {
                let degree = indegrees.get_mut(child).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    ready.push_back(child);
                }
            }
        }
    }
    if visited != indegrees.len() {
        return Err(structure(top, "combinational cycle in synthesized netlist"));
    }
    Ok(logic)
}

#[derive(Clone, Copy)]
struct Address {
    owner: i64,
    pin: i64,
}

impl Address {
    fn value(self) -> Value {
        json!({"PinOwnerID":self.owner,"PinID":self.pin})
    }
}

struct Chip {
    name: String,
    next_id: i64,
    inputs: Vec<Value>,
    outputs: Vec<Value>,
    subs: Vec<Value>,
    wires: Vec<Value>,
    positions: BTreeMap<i64, (f64, f64)>,
}

impl Chip {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            next_id: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            subs: Vec::new(),
            wires: Vec::new(),
            positions: BTreeMap::new(),
        }
    }

    fn id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn pin(&mut self, name: &str, width: usize, input: bool) -> Address {
        let id = self.id();
        let (x, y) = (
            if input { -12.0 } else { 12.0 },
            -(if input {
                self.inputs.len()
            } else {
                self.outputs.len()
            } as f64),
        );
        self.positions.insert(id, (x, y));
        let pin = json!({"Name":name,"ID":id,"Position":{"x":x,"y":y},"BitCount":width,"Colour":0,"ValueDisplayMode":if width > 1 {1} else {0}});
        if input {
            self.inputs.push(pin);
        } else {
            self.outputs.push(pin);
        }
        Address { owner: id, pin: 0 }
    }

    fn sub(&mut self, name: &str, label: &str, output_pins: &[i64]) -> i64 {
        let id = self.id();
        let index = self.subs.len();
        let (x, y) = (-8.0 + (index % 5) as f64 * 4.0, -((index / 5) as f64) * 1.5);
        self.positions.insert(id, (x, y));
        self.subs.push(json!({"Name":name,"ID":id,"Position":{"x":x,"y":y},"Label":label,"OutputPinColourInfo":output_pins.iter().map(|pin|json!({"PinColour":0,"PinID":pin})).collect::<Vec<_>>(),"InternalData":null}));
        id
    }

    fn wire(&mut self, source: Address, target: Address) {
        let (sx, sy) = self.positions[&source.owner];
        let (tx, ty) = self.positions[&target.owner];
        self.wires.push(json!({"SourcePinAddress":source.value(),"TargetPinAddress":target.value(),"ConnectionType":0,"ConnectedWireIndex":-1,"ConnectedWireSegmentIndex":-1,"Points":[{"x":sx,"y":sy},{"x":tx,"y":ty}]}));
    }

    fn nand(&mut self, a: Address, b: Address) -> Address {
        let id = self.sub("NAND", "", &[2]);
        self.wire(a, Address { owner: id, pin: 0 });
        self.wire(b, Address { owner: id, pin: 1 });
        Address { owner: id, pin: 2 }
    }

    fn not(&mut self, a: Address) -> Address {
        self.nand(a, a)
    }

    fn value(self) -> Value {
        let height = (self.inputs.len().max(self.outputs.len()) as f64 * 0.25 + 0.5).max(0.75);
        json!({"DLSVersion":VERSION,"Name":self.name,"NameLocation":0,"ChipType":0,"Size":{"x":2.25,"y":height},"Colour":{"r":0.24,"g":0.39,"b":0.55,"a":1.0},"InputPins":self.inputs,"OutputPins":self.outputs,"SubChips":self.subs,"Wires":self.wires,"Displays":[]})
    }
}

fn add_chip(chips: &mut BTreeMap<String, String>, chip: Chip) -> Result<(), ProfileError> {
    let name = chip.name.clone();
    let text = serde_json::to_string(&chip.value()).expect("DLS fields serialize");
    if text.len() > MAX_CHIP_BYTES {
        return Err(limit(
            &name,
            format!("generated chip exceeds {MAX_CHIP_BYTES} bytes"),
        ));
    }
    chips.insert(name, text);
    Ok(())
}

fn ensure_constant(
    chips: &mut BTreeMap<String, String>,
    high: bool,
) -> Result<&'static str, ProfileError> {
    let name = if high { CONST_ONE } else { CONST_ZERO };
    if chips.contains_key(name) {
        return Ok(name);
    }
    let mut c = Chip::new(name);
    let out = c.pin("OUT", 1, false);
    let source = if high {
        // Upstream NAND masks disconnected inputs to logic low, producing 1.
        let id = c.sub("NAND", "constant 1", &[2]);
        Address { owner: id, pin: 2 }
    } else {
        ensure_constant(chips, true)?;
        let id = c.sub(CONST_ONE, "", &[1]);
        c.not(Address { owner: id, pin: 1 })
    };
    c.wire(source, out);
    add_chip(chips, c)?;
    Ok(name)
}

fn reset_latch_name(reset: Option<(bool, bool)>) -> String {
    match reset {
        None => LATCH.into(),
        Some((active_high, value)) => format!(
            "{LATCH}_{}{}",
            if active_high { 'P' } else { 'N' },
            if value { '1' } else { '0' }
        ),
    }
}

fn ensure_latch(
    chips: &mut BTreeMap<String, String>,
    reset: Option<(bool, bool)>,
) -> Result<String, ProfileError> {
    let name = reset_latch_name(reset);
    if chips.contains_key(&name) {
        return Ok(name);
    }
    let mut c = Chip::new(&name);
    let d = c.pin("D", 1, true);
    let e = c.pin("E", 1, true);
    let r = reset.map(|_| c.pin("R", 1, true));
    let out = c.pin("Q", 1, false);
    let nd = c.not(d);
    let mut set_bar = c.nand(d, e);
    let mut reset_bar = c.nand(nd, e);
    if let Some((active_high, value)) = reset {
        let r = r.unwrap();
        let inactive = if active_high { c.not(r) } else { r };
        // Force one SR input low and the other high while reset is asserted,
        // independently of D and enable, for both master and slave latches.
        if value {
            let ns = c.nand(set_bar, inactive);
            set_bar = c.not(ns);
            let nr = c.not(reset_bar);
            reset_bar = c.nand(nr, inactive);
        } else {
            let ns = c.not(set_bar);
            set_bar = c.nand(ns, inactive);
            let nr = c.nand(reset_bar, inactive);
            reset_bar = c.not(nr);
        }
    }
    let q = c.sub("NAND", "Q", &[2]);
    let nq = c.sub("NAND", "Q complement", &[2]);
    c.wire(set_bar, Address { owner: q, pin: 0 });
    c.wire(Address { owner: nq, pin: 2 }, Address { owner: q, pin: 1 });
    c.wire(Address { owner: q, pin: 2 }, Address { owner: nq, pin: 0 });
    c.wire(reset_bar, Address { owner: nq, pin: 1 });
    c.wire(Address { owner: q, pin: 2 }, out);
    add_chip(chips, c)?;
    Ok(name)
}

fn ensure_sequential(
    chips: &mut BTreeMap<String, String>,
    gate: Logic,
) -> Result<String, ProfileError> {
    let name = gate.helper();
    if chips.contains_key(&name) {
        return Ok(name);
    }
    match gate {
        Logic::Latch { positive: true } => ensure_latch(chips, None),
        Logic::Latch { positive: false } => {
            ensure_latch(chips, None)?;
            let mut c = Chip::new(&name);
            let d = c.pin("D", 1, true);
            let e = c.pin("E", 1, true);
            let out = c.pin("Q", 1, false);
            let ne = c.not(e);
            let sub = c.sub(LATCH, "", &[3]);
            c.wire(d, Address { owner: sub, pin: 1 });
            c.wire(ne, Address { owner: sub, pin: 2 });
            c.wire(Address { owner: sub, pin: 3 }, out);
            add_chip(chips, c)?;
            Ok(name)
        }
        Logic::Dff { positive, reset } => {
            let latch = ensure_latch(chips, reset)?;
            let mut c = Chip::new(&name);
            let d = c.pin("D", 1, true);
            let clock = c.pin("C", 1, true);
            let r = reset.map(|_| c.pin("R", 1, true));
            let out = c.pin("Q", 1, false);
            let inverse = c.not(clock);
            let q_pin = if reset.is_some() { 4 } else { 3 };
            let master = c.sub(&latch, "master", &[q_pin]);
            let slave = c.sub(&latch, "slave", &[q_pin]);
            c.wire(
                d,
                Address {
                    owner: master,
                    pin: 1,
                },
            );
            c.wire(
                if positive { inverse } else { clock },
                Address {
                    owner: master,
                    pin: 2,
                },
            );
            c.wire(
                if positive { clock } else { inverse },
                Address {
                    owner: slave,
                    pin: 2,
                },
            );
            c.wire(
                Address {
                    owner: master,
                    pin: q_pin,
                },
                Address {
                    owner: slave,
                    pin: 1,
                },
            );
            if let Some(r) = r {
                c.wire(
                    r,
                    Address {
                        owner: master,
                        pin: 3,
                    },
                );
                c.wire(
                    r,
                    Address {
                        owner: slave,
                        pin: 3,
                    },
                );
            }
            c.wire(
                Address {
                    owner: slave,
                    pin: q_pin,
                },
                out,
            );
            add_chip(chips, c)?;
            Ok(name)
        }
        _ => unreachable!("only sequential gates require helpers"),
    }
}

fn chunks(port: &Port, name: &str) -> Vec<(String, usize, usize)> {
    let mut result = Vec::new();
    let mut start = 0;
    while start < port.bits.len() {
        let remaining = port.bits.len() - start;
        let width = if remaining >= 8 {
            8
        } else if remaining >= 4 {
            4
        } else {
            1
        };
        let index = |i: usize| {
            port.offset
                + if port.upto == 0 {
                    i as i64
                } else {
                    (port.bits.len() - 1 - i) as i64
                }
        };
        let label = if width == port.bits.len() {
            name.into()
        } else if width == 1 {
            format!("{name}[{}]", index(start))
        } else {
            format!("{name}[{}:{}]", index(start + width - 1), index(start))
        };
        result.push((label, start, width));
        start += width;
    }
    result
}

fn safe_top_name(top: &str) -> String {
    let mut name: String = top
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    name = name.trim_matches([' ', '.']).chars().take(120).collect();
    if name.is_empty() {
        name = "TOP".into();
    }
    let upper = name.to_ascii_uppercase();
    let reserved = [
        "NAND",
        "CLOCK",
        "PULSE",
        "3-STATE BUFFER",
        "KEY",
        "DEV.RAM-8",
        "ROM 256×16",
        "RGB DISPLAY",
        "DOT DISPLAY",
        "7-SEGMENT",
        "LED",
        "BUZZER",
        "4-1BIT",
        "8-1BIT",
        "8-4BIT",
        "1-4BIT",
        "1-8BIT",
        "4-8BIT",
    ];
    if reserved.contains(&upper.as_str())
        || upper.starts_with("JSONRTL_")
        || upper.starts_with("IN-")
        || upper.starts_with("OUT-")
        || upper.starts_with("BUS-")
    {
        name = format!("TOP_{name}");
    }
    name
}

fn parse_design(json_text: &str, top: &str) -> Result<Design, ProfileError> {
    if json_text.len() > MAX_JSON_BYTES {
        return Err(limit(
            top,
            format!("Yosys JSON exceeds {MAX_JSON_BYTES} bytes"),
        ));
    }
    serde_json::from_str(json_text).map_err(|error| ProfileError::Parse {
        path: "Yosys JSON".into(),
        message: error.to_string(),
    })
}

/// Check elaborated source before optimization can replace undefined states.
///
/// Run this on Yosys JSON after `hierarchy; proc; opt_clean`, before synthesis
/// and ABC. A mapped netlist alone cannot distinguish optimized X/Z branches
/// from a defined Boolean function.
pub fn validate_source_states(json_text: &str, top: &str) -> Result<(), ProfileError> {
    let design = parse_design(json_text, top)?;
    if !design.modules.contains_key(top) {
        return Err(ProfileError::UnknownUnit { unit: top.into() });
    }
    for (name, module) in &design.modules {
        validate_attributes(name, &module.attributes)?;
        for port in module.ports.values() {
            if port.direction == "inout" {
                return Err(unsupported(
                    name,
                    "inout ports cannot be represented faithfully",
                ));
            }
            validate_bits(name, &port.bits)?;
        }
        for net in module.netnames.values() {
            validate_attributes(name, &net.attributes)?;
            validate_bits(name, &net.bits)?;
        }
        for (cell_name, cell) in &module.cells {
            validate_attributes(name, &cell.attributes)?;
            for bits in cell.connections.values() {
                validate_bits(name, bits)?;
            }
            for (parameter, value) in &cell.parameters {
                if value.as_str().is_some_and(|bits| {
                    bits.bytes().all(|b| matches!(b, b'0' | b'1' | b'x' | b'z'))
                        && bits.bytes().any(|b| matches!(b, b'x' | b'z'))
                }) {
                    return Err(unsupported(
                        name,
                        format!(
                            "cell '{cell_name}' parameter '{parameter}' contains an unknown or high-impedance value"
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Convert NAND/NOT and supported bit-level register cells to native DLS files.
///
/// With `clock`, add a wrapper using DLS's built-in CLOCK for that exact
/// single-bit input port. Without it all inputs remain externally driven.
/// Source-based callers should also check [`validate_source_states`] on
/// pre-optimization JSON: mapped JSON may already contain arbitrary choices
/// made by the synthesis tool for X/Z values.
pub fn from_yosys_json(
    json_text: &str,
    top: &str,
    project_name: &str,
    clock: Option<&str>,
) -> Result<ExportProject, ProfileError> {
    let design = parse_design(json_text, top)?;
    let module = design
        .modules
        .get(top)
        .ok_or_else(|| ProfileError::UnknownUnit { unit: top.into() })?;
    let logic = validate_module(top, module)?;
    if let Some(clock) = clock {
        if !module
            .ports
            .get(clock)
            .is_some_and(|p| p.direction == "input" && p.bits.len() == 1)
        {
            return Err(unsupported(
                top,
                format!("clock '{clock}' must name an exact single-bit input port"),
            ));
        }
    }
    let top_name = safe_top_name(top);
    let mut chips = BTreeMap::new();
    let mut c = Chip::new(&top_name);
    let mut sources = BTreeMap::new();
    let mut output_targets = Vec::new();
    let mut boundaries = Vec::new();
    for (name, port) in &module.ports {
        for (label, start, width) in chunks(port, name) {
            let input = port.direction == "input";
            let pin = c.pin(&label, width, input);
            boundaries.push((name.clone(), label.clone(), pin, width, input));
            if input {
                if width == 1 {
                    sources.insert(port.bits[start].clone(), pin);
                } else {
                    let split = c.sub(
                        &format!("{width}-1BIT"),
                        &label,
                        &(1..=width as i64).collect::<Vec<_>>(),
                    );
                    c.wire(
                        pin,
                        Address {
                            owner: split,
                            pin: 0,
                        },
                    );
                    for i in 0..width {
                        sources.insert(
                            port.bits[start + i].clone(),
                            Address {
                                owner: split,
                                pin: (width - i) as i64,
                            },
                        );
                    }
                }
            } else if width == 1 {
                output_targets.push((port.bits[start].clone(), pin));
            } else {
                let merge = c.sub(&format!("1-{width}BIT"), &label, &[width as i64]);
                for i in 0..width {
                    output_targets.push((
                        port.bits[start + i].clone(),
                        Address {
                            owner: merge,
                            pin: (width - 1 - i) as i64,
                        },
                    ));
                }
                c.wire(
                    Address {
                        owner: merge,
                        pin: width as i64,
                    },
                    pin,
                );
            }
        }
    }
    let mut cell_targets = Vec::new();
    for (name, cell) in &module.cells {
        let gate = logic[name];
        let helper = if gate.sequential() {
            ensure_sequential(&mut chips, gate)?
        } else {
            "NAND".into()
        };
        let output_pin = if gate.sequential() {
            (gate.inputs().len() + 1) as i64
        } else {
            2
        };
        let sub = c.sub(&helper, name, &[output_pin]);
        sources.insert(
            cell.connections[gate.output()][0].clone(),
            Address {
                owner: sub,
                pin: output_pin,
            },
        );
        for (index, pin) in gate.inputs().iter().enumerate() {
            cell_targets.push((
                cell.connections[*pin][0].clone(),
                Address {
                    owner: sub,
                    pin: if gate.sequential() {
                        (index + 1) as i64
                    } else {
                        index as i64
                    },
                },
            ));
        }
        if matches!(gate, Logic::Not) {
            cell_targets.push((
                cell.connections["A"][0].clone(),
                Address { owner: sub, pin: 1 },
            ));
        }
    }
    for (bit, target) in output_targets.into_iter().chain(cell_targets) {
        if let Bit::Constant(state) = &bit {
            if !sources.contains_key(&bit) {
                let name = ensure_constant(&mut chips, state == "1")?;
                let sub = c.sub(name, "", &[1]);
                sources.insert(bit.clone(), Address { owner: sub, pin: 1 });
            }
        }
        let source = *sources.get(&bit).expect("validated bits have a driver");
        c.wire(source, target);
    }
    add_chip(&mut chips, c)?;
    let top_chip = if let Some(clock) = clock {
        let name = format!("{top_name}_CLOCK");
        let mut wrapper = Chip::new(&name);
        let output_ids: Vec<_> = boundaries
            .iter()
            .filter(|(_, _, _, _, input)| !*input)
            .map(|(_, _, pin, _, _)| pin.owner)
            .collect();
        let sub = wrapper.sub(&top_name, "synthesized design", &output_ids);
        let clk = wrapper.sub("CLOCK", clock, &[0]);
        for (port, label, pin, width, input) in boundaries {
            let sub_pin = Address {
                owner: sub,
                pin: pin.owner,
            };
            if input && port == clock {
                wrapper.wire(Address { owner: clk, pin: 0 }, sub_pin);
            } else {
                let boundary = wrapper.pin(&label, width, input);
                if input {
                    wrapper.wire(boundary, sub_pin);
                } else {
                    wrapper.wire(sub_pin, boundary);
                }
            }
        }
        add_chip(&mut chips, wrapper)?;
        name
    } else {
        top_name
    };
    let names: Vec<_> = chips.keys().collect();
    let project_description = serde_json::to_string_pretty(&json!({
        "ProjectName":project_name,"DLSVersion_LastSaved":VERSION,"DLSVersion_EarliestCompatible":"2.0.0",
        "CreationTime":"2000-01-01T00:00:00Z","LastSaveTime":"2000-01-01T00:00:00Z",
        "Prefs_MainPinNamesDisplayMode":2,"Prefs_ChipPinNamesDisplayMode":1,"Prefs_GridDisplayMode":1,
        "Prefs_Snapping":0,"Prefs_StraightWires":0,"Prefs_SimPaused":false,
        "Prefs_SimTargetStepsPerSecond":1000,"Prefs_SimStepsPerClockTick":250,
        "AllCustomChipNames":names,"StarredList":[{"Name":top_chip,"IsCollection":false}],
        "ChipCollections":[{"Name":"EXPORTED","Chips":names,"IsToggledOpen":true}]
    })).expect("project description serializes");
    if project_description.len() + chips.values().map(String::len).sum::<usize>()
        > MAX_PROJECT_BYTES
    {
        return Err(limit(
            top,
            format!("generated project exceeds {MAX_PROJECT_BYTES} bytes"),
        ));
    }
    Ok(ExportProject {
        top_chip,
        project_description,
        chips,
    })
}
