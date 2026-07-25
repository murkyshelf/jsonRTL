//! UI-independent parser, validator, and deterministic Verilog-2001 compiler for
//! canonical digital-logic documents.

pub mod catalog;
mod compiler;
pub mod diagnostic;
mod identifier;
mod ir;
pub mod limits;
pub mod model;
mod parser;
mod validation;

pub use catalog::{
    COMPONENT_DEFINITIONS, ComponentDefinition, LogicalPortDefinition, WidthRule,
    component_definition,
};
pub use compiler::{CompileOptions, CompileResult, SourceMap, SourceMapEntry, SourceMapKind};
pub use diagnostic::{
    DIAGNOSTIC_CODES, Diagnostic, DiagnosticCode, DiagnosticSeverity, LimitDiagnostic,
    SchemaDiagnostic, SourceReference, ValidationReport,
};
pub use identifier::VerilogIdentifier;
pub use ir::{
    NormalizedCircuit, NormalizedComponent, NormalizedConnection, NormalizedNet, NormalizedPort,
};
pub use limits::KernelLimits;
pub use model::{
    Circuit, CircuitDocument, Component, ComponentType, ConnectionMap, EditorMetadata, ModulePort,
    Net, Parameters, PortDirection, SchemaVersion,
};
pub use parser::ParseError;
pub use validation::Kernel;

/// The only canonical document version implemented by this release.
pub const SUPPORTED_SCHEMA_VERSION: &str = "1.0";

/// Semantic version of the jsonrtl library.
pub const KERNEL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The embedded canonical JSON Schema for [`SUPPORTED_SCHEMA_VERSION`].
pub const CIRCUIT_V1_SCHEMA: &str = include_str!("../../../schemas/circuit-v1.schema.json");
