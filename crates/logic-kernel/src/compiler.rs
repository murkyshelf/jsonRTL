use serde::{Deserialize, Serialize};

use crate::{
    CircuitDocument, ComponentType, Diagnostic, DiagnosticCode, DiagnosticSeverity, Kernel,
    NormalizedCircuit, NormalizedComponent, PortDirection, SourceReference, ValidationReport,
    VerilogIdentifier,
    ir::{NormalizationFailure, normalize},
};

/// Deterministic compiler options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompileOptions {
    /// Include generated-line source mappings. Enabled by default.
    pub emit_source_map: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            emit_source_map: true,
        }
    }
}

/// Structured result of a Verilog compilation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompileResult {
    pub diagnostics: ValidationReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_name: Option<VerilogIdentifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verilog: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_map: Option<SourceMap>,
}

impl CompileResult {
    /// True only when complete Verilog was produced.
    #[must_use]
    pub fn has_output(&self) -> bool {
        self.verilog.is_some()
    }
}

/// Ordered mappings from emitted statements to canonical stable IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceMap {
    entries: Vec<SourceMapEntry>,
}

impl SourceMap {
    #[must_use]
    pub fn entries(&self) -> &[SourceMapEntry] {
        &self.entries
    }
}

/// The generated construct represented by a source-map entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceMapKind {
    ModuleDeclaration,
    PortDeclaration,
    NetDeclaration,
    BoundaryAssignment,
    ComponentAssignment,
}

/// Inclusive, one-based generated line range and its canonical origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceMapEntry {
    pub kind: SourceMapKind,
    pub start_line: u32,
    pub end_line: u32,
    pub source: SourceReference,
    pub generated_identifier: VerilogIdentifier,
}

impl Kernel {
    /// Validates, normalizes, and emits deterministic synthesizable Verilog-2001.
    ///
    /// Error diagnostics always return `None` for both Verilog and source map.
    /// Warnings are preserved and do not block output.
    #[must_use]
    pub fn compile_verilog(
        &self,
        document: &CircuitDocument,
        options: &CompileOptions,
    ) -> CompileResult {
        let validation = self.validate(document);
        if validation.has_errors() {
            return CompileResult {
                diagnostics: validation,
                module_name: None,
                verilog: None,
                source_map: None,
            };
        }

        let normalized = match normalize(document) {
            Ok(normalized) => normalized,
            Err(failure) => return internal_failure(validation, failure),
        };
        let (verilog, source_map) = match emit_verilog(&normalized) {
            Ok(output) => output,
            Err(failure) => return internal_failure(validation, failure),
        };

        CompileResult {
            diagnostics: validation,
            module_name: Some(normalized.identifier().clone()),
            verilog: Some(verilog),
            source_map: options.emit_source_map.then_some(source_map),
        }
    }
}

fn internal_failure(validation: ValidationReport, failure: NormalizationFailure) -> CompileResult {
    let mut diagnostics = validation.diagnostics().to_vec();
    diagnostics.push(Diagnostic::new(
        DiagnosticCode::InternalInvariant,
        DiagnosticSeverity::Error,
        failure.message,
        *failure.source,
        Vec::new(),
        Some("Report this reproducible document as a kernel defect.".into()),
    ));
    CompileResult {
        diagnostics: ValidationReport::from_unsorted(diagnostics),
        module_name: None,
        verilog: None,
        source_map: None,
    }
}

fn emit_verilog(circuit: &NormalizedCircuit) -> Result<(String, SourceMap), NormalizationFailure> {
    let mut emitter = LineEmitter::default();
    let module_start = emitter.next_line();
    if circuit.ports().is_empty() {
        emitter.push_line(&format!("module {};", circuit.identifier()));
    } else {
        emitter.push_line(&format!("module {} (", circuit.identifier()));
        for (index, port) in circuit.ports().iter().enumerate() {
            let suffix = if index + 1 == circuit.ports().len() {
                ""
            } else {
                ","
            };
            emitter.push_line(&format!("    {}{suffix}", port.identifier()));
        }
        emitter.push_line(");");
    }
    let module_end = emitter.previous_line();
    emitter.map(
        SourceMapKind::ModuleDeclaration,
        module_start,
        module_end,
        source(circuit.original_id(), None, None, None, "name"),
        circuit.identifier().clone(),
    );

    emitter.push_line("");
    for port in circuit.ports() {
        let direction = match port.direction() {
            PortDirection::Input => "input",
            PortDirection::Output => "output",
        };
        let line = emitter.push_line(&format!(
            "{direction} wire {}{};",
            width_prefix(port.width()),
            port.identifier()
        ));
        emitter.map(
            SourceMapKind::PortDeclaration,
            line,
            line,
            source(
                circuit.original_id(),
                None,
                None,
                Some(port.original_id()),
                "declaration",
            ),
            port.identifier().clone(),
        );
    }

    emitter.push_line("");
    for net in circuit.nets() {
        let line = emitter.push_line(&format!(
            "wire {}{};",
            width_prefix(net.width()),
            net.identifier()
        ));
        emitter.map(
            SourceMapKind::NetDeclaration,
            line,
            line,
            source(
                circuit.original_id(),
                None,
                Some(net.original_id()),
                None,
                "declaration",
            ),
            net.identifier().clone(),
        );
    }

    emitter.push_line("");
    for port in circuit.ports() {
        let statement = match port.direction() {
            PortDirection::Input => {
                format!("assign {} = {};", port.net_identifier(), port.identifier())
            }
            PortDirection::Output => {
                format!("assign {} = {};", port.identifier(), port.net_identifier())
            }
        };
        let line = emitter.push_line(&statement);
        emitter.map(
            SourceMapKind::BoundaryAssignment,
            line,
            line,
            source(
                circuit.original_id(),
                None,
                Some(port.net_id()),
                Some(port.original_id()),
                match port.direction() {
                    PortDirection::Input => "inputBoundary",
                    PortDirection::Output => "outputBoundary",
                },
            ),
            port.identifier().clone(),
        );
    }

    emitter.push_line("");
    for component in circuit.components() {
        let output = connection(component, "Y", circuit.original_id())?;
        let expression = component_expression(component, circuit.original_id())?;
        let line = emitter.push_line(&format!(
            "assign {} = {expression};",
            output.net_identifier()
        ));
        emitter.map(
            SourceMapKind::ComponentAssignment,
            line,
            line,
            source(
                circuit.original_id(),
                Some(component.original_id()),
                Some(output.net_id()),
                None,
                "assignment",
            ),
            component.identifier().clone(),
        );
    }

    emitter.push_line("");
    emitter.push_line("endmodule");
    Ok((
        emitter.output,
        SourceMap {
            entries: emitter.entries,
        },
    ))
}

fn component_expression(
    component: &NormalizedComponent,
    circuit_id: &str,
) -> Result<String, NormalizationFailure> {
    let a = || connection(component, "A", circuit_id).map(|item| item.net_identifier().to_string());
    let b = || connection(component, "B", circuit_id).map(|item| item.net_identifier().to_string());
    match component.component_type() {
        ComponentType::And => Ok(format!("{} & {}", a()?, b()?)),
        ComponentType::Or => Ok(format!("{} | {}", a()?, b()?)),
        ComponentType::Xor => Ok(format!("{} ^ {}", a()?, b()?)),
        ComponentType::Xnor => Ok(format!("~({} ^ {})", a()?, b()?)),
        ComponentType::Nand => Ok(format!("~({} & {})", a()?, b()?)),
        ComponentType::Nor => Ok(format!("~({} | {})", a()?, b()?)),
        ComponentType::Not => Ok(format!("~{}", a()?)),
        ComponentType::Buffer => a(),
        ComponentType::Const => {
            let Some(value) = component.const_value() else {
                return Err(component_failure(component, circuit_id, "parameters.value"));
            };
            Ok(format!("{}'b{value}", component.width()))
        }
        ComponentType::Unknown => Err(component_failure(component, circuit_id, "type")),
    }
}

fn connection<'a>(
    component: &'a NormalizedComponent,
    logical_port: &str,
    circuit_id: &str,
) -> Result<&'a crate::NormalizedConnection, NormalizationFailure> {
    component.connection(logical_port).ok_or_else(|| {
        component_failure(
            component,
            circuit_id,
            &format!("connections.{logical_port}"),
        )
    })
}

fn component_failure(
    component: &NormalizedComponent,
    circuit_id: &str,
    field: &str,
) -> NormalizationFailure {
    NormalizationFailure {
        message: format!("normalized component violated emitter invariant at '{field}'"),
        source: Box::new(source(
            circuit_id,
            Some(component.original_id()),
            None,
            None,
            field,
        )),
    }
}

fn width_prefix(width: u32) -> String {
    if width == 1 {
        String::new()
    } else {
        format!("[{}:0] ", width - 1)
    }
}

fn source(
    circuit_id: &str,
    component_id: Option<&str>,
    net_id: Option<&str>,
    port_id: Option<&str>,
    field: &str,
) -> SourceReference {
    SourceReference {
        circuit_id: Some(circuit_id.to_owned()),
        component_id: component_id.map(str::to_owned),
        net_id: net_id.map(str::to_owned),
        port_id: port_id.map(str::to_owned),
        field: Some(field.to_owned()),
    }
}

#[derive(Debug, Default)]
struct LineEmitter {
    output: String,
    next_line: u32,
    entries: Vec<SourceMapEntry>,
}

impl LineEmitter {
    fn next_line(&self) -> u32 {
        self.next_line + 1
    }

    fn previous_line(&self) -> u32 {
        self.next_line
    }

    fn push_line(&mut self, text: &str) -> u32 {
        self.output.push_str(text);
        self.output.push('\n');
        self.next_line += 1;
        self.next_line
    }

    fn map(
        &mut self,
        kind: SourceMapKind,
        start_line: u32,
        end_line: u32,
        source: SourceReference,
        generated_identifier: VerilogIdentifier,
    ) {
        self.entries.push(SourceMapEntry {
            kind,
            start_line,
            end_line,
            source,
            generated_identifier,
        });
    }
}
