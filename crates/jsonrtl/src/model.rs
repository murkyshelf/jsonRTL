use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A version identifier from the canonical document contract.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(String);

impl SchemaVersion {
    /// Creates a schema-version value without deciding whether this kernel supports it.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the version text exactly as supplied by the document.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A canonical circuit document accepted at the kernel boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CircuitDocument {
    pub schema_version: SchemaVersion,
    pub circuit: Circuit,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_metadata: Option<EditorMetadata>,
}

/// The logical circuit. Array position is not identity or precedence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Circuit {
    pub id: String,
    pub name: String,
    pub ports: Vec<ModulePort>,
    pub components: Vec<Component>,
    pub nets: Vec<Net>,
}

/// A port at the circuit/module boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModulePort {
    pub id: String,
    pub name: String,
    pub direction: PortDirection,
    pub width: u32,
    pub net_id: String,
}

/// The direction of a module port, viewed from outside the circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortDirection {
    Input,
    Output,
}

/// A component instance from the canonical public model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub component_type: ComponentType,
    pub width: u32,
    pub connections: ConnectionMap,
    pub parameters: Parameters,
}

/// Every component type supported by canonical schema V1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComponentType {
    And,
    Or,
    Xor,
    Xnor,
    Nand,
    Nor,
    Not,
    Buffer,
    Const,
    /// Defensive typed-model value for callers that bypass schema validation.
    /// Canonical JSON Schema V1 rejects this value before deserialization.
    #[serde(other)]
    Unknown,
}

/// A contiguous bit range of one net, from schema v1.1.
///
/// `msb` and `lsb` are inclusive and index bits of the referenced net, so the
/// slice is `msb - lsb + 1` bits wide. A single bit has `msb == lsb`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetSlice {
    pub net: String,
    pub msb: u32,
    pub lsb: u32,
}

impl NetSlice {
    /// The number of bits the slice covers, or `None` when `lsb` exceeds `msb`.
    #[must_use]
    pub const fn width(&self) -> Option<u32> {
        if self.lsb > self.msb {
            None
        } else {
            Some(self.msb - self.lsb + 1)
        }
    }
}

/// What one logical component port is wired to.
///
/// Untagged so that every schema v1.0 document, in which a connection is just a
/// net ID string, keeps parsing unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Connection {
    /// The whole net, whose width must equal the component width.
    Whole(String),
    /// A contiguous bit range of a net. Schema v1.1 and later.
    Slice(NetSlice),
}

impl Connection {
    /// The referenced net ID, whether or not the connection is sliced.
    #[must_use]
    pub fn net_id(&self) -> &str {
        match self {
            Self::Whole(net_id) => net_id,
            Self::Slice(slice) => &slice.net,
        }
    }

    /// The bit range, or `None` when the whole net is referenced.
    #[must_use]
    pub const fn slice(&self) -> Option<&NetSlice> {
        match self {
            Self::Whole(_) => None,
            Self::Slice(slice) => Some(slice),
        }
    }

    /// The bits of the net this connection covers, given the net's width.
    ///
    /// Returns an empty range for a slice outside the net, which the validator
    /// reports separately as `SLICE_OUT_OF_RANGE`.
    #[must_use]
    pub fn bits(&self, net_width: u32) -> std::ops::Range<u32> {
        match self {
            Self::Whole(_) => 0..net_width,
            Self::Slice(slice) => {
                if slice.lsb > slice.msb || slice.msb >= net_width {
                    0..0
                } else {
                    slice.lsb..slice.msb + 1
                }
            }
        }
    }
}

impl From<&str> for Connection {
    fn from(net_id: &str) -> Self {
        Self::Whole(net_id.to_owned())
    }
}

impl From<String> for Connection {
    fn from(net_id: String) -> Self {
        Self::Whole(net_id)
    }
}

/// Logical component port name to the net or net slice it is wired to.
pub type ConnectionMap = BTreeMap<String, Connection>;

/// Component parameters remain JSON values until catalog-aware semantic validation.
pub type Parameters = BTreeMap<String, Value>;

/// A logical net. Drivers and sinks are derived from port and component references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Net {
    pub id: String,
    pub name: String,
    pub width: u32,
}

/// Opaque UI-owned data that is excluded from all logical semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EditorMetadata(pub Value);
