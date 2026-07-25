use std::sync::OnceLock;

use jsonschema::Validator;
use serde_json::Value;
use thiserror::Error;

use crate::{
    CIRCUIT_V1_SCHEMA, CircuitDocument, KernelLimits, LimitDiagnostic, SUPPORTED_SCHEMA_VERSIONS,
    SchemaDiagnostic,
};

static SCHEMA_VALIDATOR: OnceLock<Result<Validator, String>> = OnceLock::new();

/// Failure returned while converting untrusted JSON into the public model.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("document is {actual} bytes; configured maximum is {maximum} bytes")]
    DocumentTooLarge { actual: usize, maximum: usize },

    #[error("malformed JSON at line {line}, column {column}: {message}")]
    MalformedJson {
        message: String,
        line: usize,
        column: usize,
    },

    #[error("unsupported schema version '{found}'; supported versions are {}", supported.join(", "))]
    UnsupportedSchemaVersion {
        found: String,
        supported: &'static [&'static str],
    },

    #[error("document does not satisfy the canonical circuit schema")]
    Schema { diagnostics: Vec<SchemaDiagnostic> },

    #[error("document exceeds configured kernel limits")]
    ResourceLimits { diagnostics: Vec<LimitDiagnostic> },

    #[error("embedded canonical schema is invalid: {message}")]
    InvalidEmbeddedSchema { message: String },

    #[error("schema-valid document could not be deserialized: {message}")]
    Deserialization { message: String },
}

impl CircuitDocument {
    /// Parses with [`KernelLimits::default`].
    pub fn from_json(input: &str) -> Result<Self, ParseError> {
        Self::from_json_with_limits(input, &KernelLimits::default())
    }

    /// Parses untrusted canonical JSON with explicit deployment limits.
    pub fn from_json_with_limits(input: &str, limits: &KernelLimits) -> Result<Self, ParseError> {
        if input.len() > limits.max_document_bytes {
            return Err(ParseError::DocumentTooLarge {
                actual: input.len(),
                maximum: limits.max_document_bytes,
            });
        }

        let value: Value =
            serde_json::from_str(input).map_err(|error| ParseError::MalformedJson {
                message: error.to_string(),
                line: error.line(),
                column: error.column(),
            })?;

        if let Some(version) = value.get("schemaVersion").and_then(Value::as_str) {
            if !SUPPORTED_SCHEMA_VERSIONS.contains(&version) {
                return Err(ParseError::UnsupportedSchemaVersion {
                    found: version.to_owned(),
                    supported: SUPPORTED_SCHEMA_VERSIONS,
                });
            }
        }

        let validator = schema_validator()?;
        let mut diagnostics: Vec<_> = validator
            .iter_errors(&value)
            .map(|error| SchemaDiagnostic {
                code: schema_code(error.kind().keyword()).to_owned(),
                json_path: error.instance_path().to_string(),
                schema_path: error.schema_path().to_string(),
                message: error.to_string(),
            })
            .collect();

        diagnostics.sort_by(|left, right| {
            (
                &left.json_path,
                &left.code,
                &left.schema_path,
                &left.message,
            )
                .cmp(&(
                    &right.json_path,
                    &right.code,
                    &right.schema_path,
                    &right.message,
                ))
        });

        if !diagnostics.is_empty() {
            return Err(ParseError::Schema { diagnostics });
        }

        let document: Self =
            serde_json::from_value(value).map_err(|error| ParseError::Deserialization {
                message: error.to_string(),
            })?;

        let diagnostics = limit_diagnostics(&document, limits);
        if !diagnostics.is_empty() {
            return Err(ParseError::ResourceLimits { diagnostics });
        }

        Ok(document)
    }
}

fn schema_validator() -> Result<&'static Validator, ParseError> {
    let result = SCHEMA_VALIDATOR.get_or_init(|| {
        let schema: Value = serde_json::from_str(CIRCUIT_V1_SCHEMA)
            .map_err(|error| format!("schema JSON is malformed: {error}"))?;
        jsonschema::draft202012::new(&schema).map_err(|error| error.to_string())
    });

    result
        .as_ref()
        .map_err(|message| ParseError::InvalidEmbeddedSchema {
            message: message.clone(),
        })
}

fn schema_code(keyword: &str) -> &'static str {
    match keyword {
        "additionalProperties" | "unevaluatedProperties" => "SCHEMA_UNKNOWN_FIELD",
        "enum" | "const" => "SCHEMA_INVALID_VALUE",
        "maximum" | "maxItems" | "maxLength" | "maxProperties" => "SCHEMA_MAXIMUM",
        "minimum" | "minItems" | "minLength" | "minProperties" => "SCHEMA_MINIMUM",
        "pattern" | "propertyNames" => "SCHEMA_PATTERN",
        "required" => "SCHEMA_REQUIRED_FIELD",
        "type" => "SCHEMA_TYPE",
        _ => "SCHEMA_INVALID",
    }
}

fn limit_diagnostics(document: &CircuitDocument, limits: &KernelLimits) -> Vec<LimitDiagnostic> {
    let mut diagnostics = Vec::new();
    let circuit = &document.circuit;

    push_limit(
        &mut diagnostics,
        "LIMIT_PORTS",
        "/circuit/ports",
        circuit.ports.len(),
        limits.max_ports,
        "module ports",
    );
    push_limit(
        &mut diagnostics,
        "LIMIT_COMPONENTS",
        "/circuit/components",
        circuit.components.len(),
        limits.max_components,
        "components",
    );
    push_limit(
        &mut diagnostics,
        "LIMIT_NETS",
        "/circuit/nets",
        circuit.nets.len(),
        limits.max_nets,
        "nets",
    );

    check_string(
        &mut diagnostics,
        "/circuit/id",
        &circuit.id,
        limits.max_string_length,
    );
    check_string(
        &mut diagnostics,
        "/circuit/name",
        &circuit.name,
        limits.max_string_length,
    );

    for (index, port) in circuit.ports.iter().enumerate() {
        check_string(
            &mut diagnostics,
            &format!("/circuit/ports/{index}/id"),
            &port.id,
            limits.max_string_length,
        );
        check_string(
            &mut diagnostics,
            &format!("/circuit/ports/{index}/name"),
            &port.name,
            limits.max_string_length,
        );
        check_string(
            &mut diagnostics,
            &format!("/circuit/ports/{index}/netId"),
            &port.net_id,
            limits.max_string_length,
        );
        check_width(
            &mut diagnostics,
            &format!("/circuit/ports/{index}/width"),
            port.width,
            limits.max_width,
        );
    }

    for (index, component) in circuit.components.iter().enumerate() {
        check_string(
            &mut diagnostics,
            &format!("/circuit/components/{index}/id"),
            &component.id,
            limits.max_string_length,
        );
        check_string(
            &mut diagnostics,
            &format!("/circuit/components/{index}/name"),
            &component.name,
            limits.max_string_length,
        );
        check_width(
            &mut diagnostics,
            &format!("/circuit/components/{index}/width"),
            component.width,
            limits.max_width,
        );
        push_limit(
            &mut diagnostics,
            "LIMIT_PARAMETERS",
            &format!("/circuit/components/{index}/parameters"),
            component.parameters.len(),
            limits.max_parameters_per_component,
            "component parameters",
        );
        for (port_name, connection) in &component.connections {
            check_string(
                &mut diagnostics,
                &format!("/circuit/components/{index}/connections/{port_name}"),
                port_name,
                limits.max_string_length,
            );
            check_string(
                &mut diagnostics,
                &format!("/circuit/components/{index}/connections/{port_name}"),
                connection.net_id(),
                limits.max_string_length,
            );
        }
        for parameter_name in component.parameters.keys() {
            check_string(
                &mut diagnostics,
                &format!("/circuit/components/{index}/parameters/{parameter_name}"),
                parameter_name,
                limits.max_string_length,
            );
        }
    }

    for (index, net) in circuit.nets.iter().enumerate() {
        check_string(
            &mut diagnostics,
            &format!("/circuit/nets/{index}/id"),
            &net.id,
            limits.max_string_length,
        );
        check_string(
            &mut diagnostics,
            &format!("/circuit/nets/{index}/name"),
            &net.name,
            limits.max_string_length,
        );
        check_width(
            &mut diagnostics,
            &format!("/circuit/nets/{index}/width"),
            net.width,
            limits.max_width,
        );
    }

    diagnostics.sort_by(|left, right| {
        (&left.json_path, &left.code, left.actual, left.maximum).cmp(&(
            &right.json_path,
            &right.code,
            right.actual,
            right.maximum,
        ))
    });
    diagnostics
}

fn check_string(diagnostics: &mut Vec<LimitDiagnostic>, path: &str, value: &str, maximum: usize) {
    push_limit(
        diagnostics,
        "LIMIT_STRING_LENGTH",
        path,
        value.chars().count(),
        maximum,
        "Unicode scalar values",
    );
}

fn check_width(diagnostics: &mut Vec<LimitDiagnostic>, path: &str, width: u32, maximum: u32) {
    push_limit(
        diagnostics,
        "LIMIT_WIDTH",
        path,
        width as usize,
        maximum as usize,
        "bits",
    );
}

fn push_limit(
    diagnostics: &mut Vec<LimitDiagnostic>,
    code: &str,
    path: &str,
    actual: usize,
    maximum: usize,
    unit: &str,
) {
    if actual > maximum {
        diagnostics.push(LimitDiagnostic {
            code: code.to_owned(),
            json_path: path.to_owned(),
            actual,
            maximum,
            message: format!("{actual} {unit} exceeds configured maximum {maximum}"),
        });
    }
}
