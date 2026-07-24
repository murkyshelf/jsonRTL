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

/// Logical component port name to canonical net ID.
pub type ConnectionMap = BTreeMap<String, String>;

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
