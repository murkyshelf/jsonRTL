use serde::{Deserialize, Serialize};

/// A stable JSON Schema failure reported by the canonical parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaDiagnostic {
    pub code: String,
    pub json_path: String,
    pub schema_path: String,
    pub message: String,
}

/// A configurable resource-limit failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LimitDiagnostic {
    pub code: String,
    pub json_path: String,
    pub actual: usize,
    pub maximum: usize,
    pub message: String,
}

/// Stable semantic diagnostic codes. Meanings are documented in
/// `docs/diagnostics.md` and must never be silently repurposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiagnosticCode {
    IdDuplicateComponent,
    IdDuplicateNet,
    IdDuplicatePort,
    NameDuplicatePort,
    NameEmpty,
    NameInvalid,
    NameRequiresSanitization,
    NameVerilogKeyword,
    NameSanitizationCollision,
    NetUnknownReference,
    ComponentMissingConnection,
    ComponentUnknownConnection,
    ComponentUnknownType,
    ComponentMissingParameter,
    ComponentUnknownParameter,
    WidthZero,
    WidthExceedsLimit,
    WidthPortNetMismatch,
    WidthComponentNetMismatch,
    ConstLiteralMalformed,
    ConstValueWidthMismatch,
    NetMultipleDrivers,
    NetNoDriver,
    NetNoConsumers,
    NetUnused,
    GraphCombinationalCycle,
    LimitPorts,
    LimitComponents,
    LimitNets,
    LimitParameters,
    LimitStringLength,
    InternalInvariant,
}

/// Complete Phase 2 semantic diagnostic registry.
pub const DIAGNOSTIC_CODES: &[DiagnosticCode] = &[
    DiagnosticCode::IdDuplicateComponent,
    DiagnosticCode::IdDuplicateNet,
    DiagnosticCode::IdDuplicatePort,
    DiagnosticCode::NameDuplicatePort,
    DiagnosticCode::NameEmpty,
    DiagnosticCode::NameInvalid,
    DiagnosticCode::NameRequiresSanitization,
    DiagnosticCode::NameVerilogKeyword,
    DiagnosticCode::NameSanitizationCollision,
    DiagnosticCode::NetUnknownReference,
    DiagnosticCode::ComponentMissingConnection,
    DiagnosticCode::ComponentUnknownConnection,
    DiagnosticCode::ComponentUnknownType,
    DiagnosticCode::ComponentMissingParameter,
    DiagnosticCode::ComponentUnknownParameter,
    DiagnosticCode::WidthZero,
    DiagnosticCode::WidthExceedsLimit,
    DiagnosticCode::WidthPortNetMismatch,
    DiagnosticCode::WidthComponentNetMismatch,
    DiagnosticCode::ConstLiteralMalformed,
    DiagnosticCode::ConstValueWidthMismatch,
    DiagnosticCode::NetMultipleDrivers,
    DiagnosticCode::NetNoDriver,
    DiagnosticCode::NetNoConsumers,
    DiagnosticCode::NetUnused,
    DiagnosticCode::GraphCombinationalCycle,
    DiagnosticCode::LimitPorts,
    DiagnosticCode::LimitComponents,
    DiagnosticCode::LimitNets,
    DiagnosticCode::LimitParameters,
    DiagnosticCode::LimitStringLength,
    DiagnosticCode::InternalInvariant,
];

impl DiagnosticCode {
    /// Returns the stable serialized spelling of this code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IdDuplicateComponent => "ID_DUPLICATE_COMPONENT",
            Self::IdDuplicateNet => "ID_DUPLICATE_NET",
            Self::IdDuplicatePort => "ID_DUPLICATE_PORT",
            Self::NameDuplicatePort => "NAME_DUPLICATE_PORT",
            Self::NameEmpty => "NAME_EMPTY",
            Self::NameInvalid => "NAME_INVALID",
            Self::NameRequiresSanitization => "NAME_REQUIRES_SANITIZATION",
            Self::NameVerilogKeyword => "NAME_VERILOG_KEYWORD",
            Self::NameSanitizationCollision => "NAME_SANITIZATION_COLLISION",
            Self::NetUnknownReference => "NET_UNKNOWN_REFERENCE",
            Self::ComponentMissingConnection => "COMPONENT_MISSING_CONNECTION",
            Self::ComponentUnknownConnection => "COMPONENT_UNKNOWN_CONNECTION",
            Self::ComponentUnknownType => "COMPONENT_UNKNOWN_TYPE",
            Self::ComponentMissingParameter => "COMPONENT_MISSING_PARAMETER",
            Self::ComponentUnknownParameter => "COMPONENT_UNKNOWN_PARAMETER",
            Self::WidthZero => "WIDTH_ZERO",
            Self::WidthExceedsLimit => "WIDTH_EXCEEDS_LIMIT",
            Self::WidthPortNetMismatch => "WIDTH_PORT_NET_MISMATCH",
            Self::WidthComponentNetMismatch => "WIDTH_COMPONENT_NET_MISMATCH",
            Self::ConstLiteralMalformed => "CONST_LITERAL_MALFORMED",
            Self::ConstValueWidthMismatch => "CONST_VALUE_WIDTH_MISMATCH",
            Self::NetMultipleDrivers => "NET_MULTIPLE_DRIVERS",
            Self::NetNoDriver => "NET_NO_DRIVER",
            Self::NetNoConsumers => "NET_NO_CONSUMERS",
            Self::NetUnused => "NET_UNUSED",
            Self::GraphCombinationalCycle => "GRAPH_COMBINATIONAL_CYCLE",
            Self::LimitPorts => "LIMIT_PORTS",
            Self::LimitComponents => "LIMIT_COMPONENTS",
            Self::LimitNets => "LIMIT_NETS",
            Self::LimitParameters => "LIMIT_PARAMETERS",
            Self::LimitStringLength => "LIMIT_STRING_LENGTH",
            Self::InternalInvariant => "INTERNAL_INVARIANT",
        }
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Whether a semantic issue blocks later compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

impl DiagnosticSeverity {
    pub(crate) const fn rank(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Info => 2,
        }
    }
}

/// Stable source identity in the original canonical document.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circuit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// One semantic validation result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: SourceReference,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_sources: Vec<SourceReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    pub ordering_key: String,
}

impl Diagnostic {
    pub(crate) fn new(
        code: DiagnosticCode,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
        source: SourceReference,
        mut related_sources: Vec<SourceReference>,
        help: Option<String>,
    ) -> Self {
        related_sources.sort();
        related_sources.dedup();
        let message = message.into();
        let ordering_key = ordering_key(severity, code, &source, &message);
        Self {
            code,
            severity,
            message,
            source,
            related_sources,
            help,
            ordering_key,
        }
    }

    pub(crate) fn compare(left: &Self, right: &Self) -> std::cmp::Ordering {
        (
            left.severity.rank(),
            left.code.as_str(),
            &left.source.circuit_id,
            &left.source.component_id,
            &left.source.net_id,
            &left.source.port_id,
            &left.source.field,
            &left.message,
            &left.related_sources,
        )
            .cmp(&(
                right.severity.rank(),
                right.code.as_str(),
                &right.source.circuit_id,
                &right.source.component_id,
                &right.source.net_id,
                &right.source.port_id,
                &right.source.field,
                &right.message,
                &right.related_sources,
            ))
    }
}

/// Deterministically ordered semantic validation results.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationReport {
    diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    pub(crate) fn from_unsorted(mut diagnostics: Vec<Diagnostic>) -> Self {
        diagnostics.sort_by(Diagnostic::compare);
        diagnostics.dedup();
        for (index, diagnostic) in diagnostics.iter_mut().enumerate() {
            diagnostic.ordering_key = format!("{index:08}|{}", diagnostic.ordering_key);
        }
        Self { diagnostics }
    }

    /// All diagnostics in deterministic order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// True when at least one diagnostic blocks compilation.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|item| item.severity == DiagnosticSeverity::Error)
    }

    /// Iterates over error diagnostics without reallocating.
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|item| item.severity == DiagnosticSeverity::Error)
    }

    /// Iterates over warning diagnostics without reallocating.
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|item| item.severity == DiagnosticSeverity::Warning)
    }
}

fn ordering_key(
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    source: &SourceReference,
    message: &str,
) -> String {
    let parts = [
        source.circuit_id.as_deref().unwrap_or_default(),
        source.component_id.as_deref().unwrap_or_default(),
        source.net_id.as_deref().unwrap_or_default(),
        source.port_id.as_deref().unwrap_or_default(),
        source.field.as_deref().unwrap_or_default(),
        message,
    ];
    let encoded = parts
        .iter()
        .map(|part| format!("{}#{part}", part.len()))
        .collect::<Vec<_>>()
        .join("|");
    format!("{}|{}|{encoded}", severity.rank(), code.as_str())
}
