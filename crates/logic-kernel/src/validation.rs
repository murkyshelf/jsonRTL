use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Circuit, CircuitDocument, Component, ComponentType, Diagnostic, DiagnosticCode,
    DiagnosticSeverity, KernelLimits, ModulePort, Net, PortDirection, SourceReference,
    ValidationReport, component_definition,
};

/// Stateless validation facade configured with trusted resource limits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Kernel {
    limits: KernelLimits,
}

impl Kernel {
    /// Creates a kernel with explicit semantic-boundary limits.
    #[must_use]
    pub const fn new(limits: KernelLimits) -> Self {
        Self { limits }
    }

    /// Returns this kernel's configured resource limits.
    #[must_use]
    pub const fn limits(&self) -> &KernelLimits {
        &self.limits
    }

    /// Collects independent semantic problems in deterministic order.
    ///
    /// Count and parameter-limit failures stop deeper validation because proceeding
    /// would let an untrusted typed model control allocation and diagnostic volume.
    #[must_use]
    pub fn validate(&self, document: &CircuitDocument) -> ValidationReport {
        let mut diagnostics = preflight_limit_diagnostics(document, &self.limits);
        if !diagnostics.is_empty() {
            return ValidationReport::from_unsorted(diagnostics);
        }

        let index = DocumentIndex::new(&document.circuit);
        validate_identity(document, &index, &mut diagnostics);
        validate_names(document, &self.limits, &mut diagnostics);
        validate_width_defenses(document, &self.limits, &mut diagnostics);
        validate_references_and_catalog(document, &index, &mut diagnostics);

        let roles = build_electrical_roles(document, &index);
        validate_electrical_roles(document, &roles, &mut diagnostics);

        if index.duplicate_component_ids.is_empty() && index.duplicate_net_ids.is_empty() {
            validate_cycles(document, &index, &roles, &mut diagnostics);
        }

        ValidationReport::from_unsorted(diagnostics)
    }
}

struct DocumentIndex<'a> {
    all_net_ids: BTreeSet<&'a str>,
    unique_nets: BTreeMap<&'a str, &'a Net>,
    duplicate_component_ids: BTreeSet<&'a str>,
    duplicate_net_ids: BTreeSet<&'a str>,
    duplicate_port_ids: BTreeSet<&'a str>,
}

impl<'a> DocumentIndex<'a> {
    fn new(circuit: &'a Circuit) -> Self {
        let component_groups = groups_by_id(&circuit.components, |item| item.id.as_str());
        let net_groups = groups_by_id(&circuit.nets, |item| item.id.as_str());
        let port_groups = groups_by_id(&circuit.ports, |item| item.id.as_str());

        let duplicate_component_ids = duplicate_keys(&component_groups);
        let duplicate_net_ids = duplicate_keys(&net_groups);
        let duplicate_port_ids = duplicate_keys(&port_groups);
        let all_net_ids = net_groups.keys().copied().collect();
        let unique_nets = net_groups
            .into_iter()
            .filter_map(|(id, items)| (items.len() == 1).then_some((id, items[0])))
            .collect();

        Self {
            all_net_ids,
            unique_nets,
            duplicate_component_ids,
            duplicate_net_ids,
            duplicate_port_ids,
        }
    }
}

fn groups_by_id<'a, T>(
    items: &'a [T],
    id: impl Fn(&'a T) -> &'a str,
) -> BTreeMap<&'a str, Vec<&'a T>> {
    let mut groups = BTreeMap::new();
    for item in items {
        groups.entry(id(item)).or_insert_with(Vec::new).push(item);
    }
    groups
}

fn duplicate_keys<'a, T>(groups: &BTreeMap<&'a str, Vec<&T>>) -> BTreeSet<&'a str> {
    groups
        .iter()
        .filter_map(|(key, items)| (items.len() > 1).then_some(*key))
        .collect()
}

fn preflight_limit_diagnostics(
    document: &CircuitDocument,
    limits: &KernelLimits,
) -> Vec<Diagnostic> {
    let circuit = &document.circuit;
    let mut diagnostics = Vec::new();

    push_count_limit(
        &mut diagnostics,
        document,
        DiagnosticCode::LimitPorts,
        "ports",
        circuit.ports.len(),
        limits.max_ports,
    );
    push_count_limit(
        &mut diagnostics,
        document,
        DiagnosticCode::LimitComponents,
        "components",
        circuit.components.len(),
        limits.max_components,
    );
    push_count_limit(
        &mut diagnostics,
        document,
        DiagnosticCode::LimitNets,
        "nets",
        circuit.nets.len(),
        limits.max_nets,
    );

    if !diagnostics.is_empty() {
        return diagnostics;
    }

    for component in &circuit.components {
        if component.parameters.len() > limits.max_parameters_per_component {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::LimitParameters,
                DiagnosticSeverity::Error,
                format!(
                    "Component '{}' has {} parameters; configured maximum is {}.",
                    component.id,
                    component.parameters.len(),
                    limits.max_parameters_per_component
                ),
                component_source(document, component, "parameters"),
                Vec::new(),
                Some("Remove unsupported parameters or raise the trusted deployment limit.".into()),
            ));
        }
    }

    diagnostics
}

fn push_count_limit(
    diagnostics: &mut Vec<Diagnostic>,
    document: &CircuitDocument,
    code: DiagnosticCode,
    field: &str,
    actual: usize,
    maximum: usize,
) {
    if actual > maximum {
        diagnostics.push(Diagnostic::new(
            code,
            DiagnosticSeverity::Error,
            format!("Circuit has {actual} {field}; configured maximum is {maximum}."),
            circuit_source(document, field),
            Vec::new(),
            Some("Reduce the circuit or raise the trusted deployment limit.".into()),
        ));
    }
}

fn validate_identity(
    document: &CircuitDocument,
    index: &DocumentIndex<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for id in &index.duplicate_component_ids {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::IdDuplicateComponent,
            DiagnosticSeverity::Error,
            format!("Component ID '{id}' is declared more than once."),
            source(document, Some(id), None, None, "id"),
            Vec::new(),
            Some("Assign a distinct stable ID to every component.".into()),
        ));
    }
    for id in &index.duplicate_net_ids {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::IdDuplicateNet,
            DiagnosticSeverity::Error,
            format!("Net ID '{id}' is declared more than once."),
            source(document, None, Some(id), None, "id"),
            Vec::new(),
            Some("Assign a distinct stable ID to every net.".into()),
        ));
    }
    for id in &index.duplicate_port_ids {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::IdDuplicatePort,
            DiagnosticSeverity::Error,
            format!("Module-port ID '{id}' is declared more than once."),
            source(document, None, None, Some(id), "id"),
            Vec::new(),
            Some("Assign a distinct stable ID to every module port.".into()),
        ));
    }

    let mut names: BTreeMap<&str, Vec<&ModulePort>> = BTreeMap::new();
    for port in &document.circuit.ports {
        names.entry(&port.name).or_default().push(port);
    }
    for (name, ports) in names.into_iter().filter(|(_, ports)| ports.len() > 1) {
        let mut sources: Vec<_> = ports
            .iter()
            .map(|port| port_source(document, port, "name"))
            .collect();
        sources.sort();
        let primary = sources.remove(0);
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::NameDuplicatePort,
            DiagnosticSeverity::Error,
            format!("External port name '{name}' is declared more than once."),
            primary,
            sources,
            Some("Give every external port a distinct name.".into()),
        ));
    }
}

fn validate_names(
    document: &CircuitDocument,
    limits: &KernelLimits,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_one_name(
        document,
        "circuit",
        &document.circuit.name,
        circuit_source(document, "name"),
        limits,
        diagnostics,
    );

    let mut port_names = Vec::new();
    for port in &document.circuit.ports {
        let item_source = port_source(document, port, "name");
        validate_one_name(
            document,
            "module port",
            &port.name,
            item_source.clone(),
            limits,
            diagnostics,
        );
        validate_string_limit(
            document,
            &port.id,
            port_source(document, port, "id"),
            limits,
            diagnostics,
        );
        validate_string_limit(
            document,
            &port.net_id,
            port_source(document, port, "netId"),
            limits,
            diagnostics,
        );
        port_names.push((port.name.as_str(), item_source));
    }

    let mut component_names = Vec::new();
    for component in &document.circuit.components {
        let item_source = component_source(document, component, "name");
        validate_one_name(
            document,
            "component",
            &component.name,
            item_source.clone(),
            limits,
            diagnostics,
        );
        validate_string_limit(
            document,
            &component.id,
            component_source(document, component, "id"),
            limits,
            diagnostics,
        );
        // V1 catalog entries have at most three ports. Inspect at most one extra
        // entry here so a typed model that bypassed the schema cannot drive
        // unbounded diagnostic allocation through a huge connection map.
        for (logical_port, net_id) in component.connections.iter().take(4) {
            validate_string_limit(
                document,
                logical_port,
                component_source(document, component, &format!("connections.{logical_port}")),
                limits,
                diagnostics,
            );
            validate_string_limit(
                document,
                net_id,
                component_source(document, component, &format!("connections.{logical_port}")),
                limits,
                diagnostics,
            );
        }
        for parameter in component.parameters.keys() {
            validate_string_limit(
                document,
                parameter,
                component_source(document, component, &format!("parameters.{parameter}")),
                limits,
                diagnostics,
            );
        }
        component_names.push((component.name.as_str(), item_source));
    }

    let mut net_names = Vec::new();
    for net in &document.circuit.nets {
        let item_source = net_source(document, net, "name");
        validate_one_name(
            document,
            "net",
            &net.name,
            item_source.clone(),
            limits,
            diagnostics,
        );
        validate_string_limit(
            document,
            &net.id,
            net_source(document, net, "id"),
            limits,
            diagnostics,
        );
        net_names.push((net.name.as_str(), item_source));
    }

    validate_string_limit(
        document,
        &document.circuit.id,
        circuit_source(document, "id"),
        limits,
        diagnostics,
    );

    validate_sanitization_collisions("external ports", port_names, true, diagnostics);
    validate_sanitization_collisions("components", component_names, false, diagnostics);
    validate_sanitization_collisions("nets", net_names, false, diagnostics);
}

fn validate_one_name(
    document: &CircuitDocument,
    kind: &str,
    name: &str,
    item_source: SourceReference,
    limits: &KernelLimits,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_string_limit(document, name, item_source.clone(), limits, diagnostics);

    if name.trim().is_empty() {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::NameEmpty,
            DiagnosticSeverity::Error,
            format!("The {kind} name is empty or whitespace-only."),
            item_source,
            Vec::new(),
            Some("Provide a non-empty display name.".into()),
        ));
        return;
    }
    if name.chars().any(char::is_control) {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::NameInvalid,
            DiagnosticSeverity::Error,
            format!("The {kind} name contains a control character."),
            item_source,
            Vec::new(),
            Some("Remove control characters from the display name.".into()),
        ));
        return;
    }

    let stem = crate::identifier::sanitized_stem(name);
    let sanitized = crate::identifier::sanitize_identifier_candidate(name);
    if sanitized != name {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::NameRequiresSanitization,
            DiagnosticSeverity::Warning,
            format!("The {kind} name '{name}' will sanitize to '{sanitized}'."),
            item_source.clone(),
            Vec::new(),
            Some("Use an ASCII Verilog identifier to avoid renaming.".into()),
        ));
    }
    if crate::identifier::is_verilog_keyword(&stem) {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::NameVerilogKeyword,
            DiagnosticSeverity::Warning,
            format!("The {kind} name '{name}' is a Verilog-2001 keyword."),
            item_source,
            Vec::new(),
            Some("Choose a non-keyword name; generation will also resolve it safely.".into()),
        ));
    }
}

fn validate_string_limit(
    _document: &CircuitDocument,
    value: &str,
    item_source: SourceReference,
    limits: &KernelLimits,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let actual = value.chars().count();
    if actual > limits.max_string_length {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::LimitStringLength,
            DiagnosticSeverity::Error,
            format!(
                "String has {actual} Unicode scalar values; configured maximum is {}.",
                limits.max_string_length
            ),
            item_source,
            Vec::new(),
            Some("Shorten the ID, name, reference, or key.".into()),
        ));
    }
}

fn validate_sanitization_collisions(
    namespace: &str,
    items: Vec<(&str, SourceReference)>,
    skip_exact_duplicates: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut groups: BTreeMap<String, Vec<(&str, SourceReference)>> = BTreeMap::new();
    for (name, item_source) in items {
        groups
            .entry(crate::identifier::sanitize_identifier_candidate(name))
            .or_default()
            .push((name, item_source));
    }

    for (sanitized, mut entries) in groups.into_iter().filter(|(_, entries)| entries.len() > 1) {
        let originals: BTreeSet<_> = entries.iter().map(|(name, _)| *name).collect();
        if skip_exact_duplicates && originals.len() == 1 {
            continue;
        }
        entries.sort_by(|left, right| left.1.cmp(&right.1));
        let primary = entries.remove(0).1;
        let related = entries.into_iter().map(|(_, item)| item).collect();
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::NameSanitizationCollision,
            DiagnosticSeverity::Warning,
            format!("Multiple {namespace} sanitize to Verilog identifier '{sanitized}'."),
            primary,
            related,
            Some("Choose names that remain distinct after Verilog sanitization.".into()),
        ));
    }
}

fn validate_width_defenses(
    document: &CircuitDocument,
    limits: &KernelLimits,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for port in &document.circuit.ports {
        validate_width(
            port.width,
            limits,
            port_source(document, port, "width"),
            diagnostics,
        );
    }
    for component in &document.circuit.components {
        validate_width(
            component.width,
            limits,
            component_source(document, component, "width"),
            diagnostics,
        );
    }
    for net in &document.circuit.nets {
        validate_width(
            net.width,
            limits,
            net_source(document, net, "width"),
            diagnostics,
        );
    }
}

fn validate_width(
    width: u32,
    limits: &KernelLimits,
    item_source: SourceReference,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if width == 0 {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::WidthZero,
            DiagnosticSeverity::Error,
            "Width must be at least one bit.",
            item_source,
            Vec::new(),
            Some("Set width to a positive integer.".into()),
        ));
    } else if width > limits.max_width {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::WidthExceedsLimit,
            DiagnosticSeverity::Error,
            format!(
                "Width {width} exceeds the configured maximum {}.",
                limits.max_width
            ),
            item_source,
            Vec::new(),
            Some("Reduce the width or raise the trusted deployment limit.".into()),
        ));
    }
}

fn validate_references_and_catalog(
    document: &CircuitDocument,
    index: &DocumentIndex<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for port in &document.circuit.ports {
        if !index.all_net_ids.contains(port.net_id.as_str()) {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::NetUnknownReference,
                DiagnosticSeverity::Error,
                format!(
                    "Module port '{}' references unknown net '{}'.",
                    port.id, port.net_id
                ),
                port_source_with_net(document, port, "netId", &port.net_id),
                Vec::new(),
                Some("Connect the port to a declared net ID.".into()),
            ));
        } else if let Some(net) = index.unique_nets.get(port.net_id.as_str()) {
            if port.width != net.width {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::WidthPortNetMismatch,
                    DiagnosticSeverity::Error,
                    format!(
                        "Module port '{}' width {} does not match net '{}' width {}.",
                        port.id, port.width, net.id, net.width
                    ),
                    port_source_with_net(document, port, "width", &net.id),
                    vec![net_source(document, net, "width")],
                    Some("Use identical widths; V1 has no implicit conversion.".into()),
                ));
            }
        }
    }

    for component in &document.circuit.components {
        let Some(definition) = component_definition(component.component_type) else {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::ComponentUnknownType,
                DiagnosticSeverity::Error,
                format!(
                    "Component '{}' has a type outside the V1 catalog.",
                    component.id
                ),
                component_source(document, component, "type"),
                Vec::new(),
                Some("Use one of the nine canonical V1 component types.".into()),
            ));
            continue;
        };

        for logical_port in definition.ports {
            let field = format!("connections.{}", logical_port.name);
            let Some(net_id) = component.connections.get(logical_port.name) else {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::ComponentMissingConnection,
                    DiagnosticSeverity::Error,
                    format!(
                        "Component '{}' is missing required logical connection '{}'.",
                        component.id, logical_port.name
                    ),
                    component_source(document, component, &field),
                    Vec::new(),
                    Some(format!(
                        "Connect logical port '{}' to a net ID.",
                        logical_port.name
                    )),
                ));
                continue;
            };

            if !index.all_net_ids.contains(net_id.as_str()) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::NetUnknownReference,
                    DiagnosticSeverity::Error,
                    format!(
                        "Component '{}' logical port '{}' references unknown net '{}'.",
                        component.id, logical_port.name, net_id
                    ),
                    component_source_with_net(document, component, &field, net_id),
                    Vec::new(),
                    Some("Connect the logical port to a declared net ID.".into()),
                ));
            } else if let Some(net) = index.unique_nets.get(net_id.as_str()) {
                if component.width != net.width {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::WidthComponentNetMismatch,
                        DiagnosticSeverity::Error,
                        format!(
                            "Component '{}' logical port '{}' width {} does not match net '{}' width {}.",
                            component.id, logical_port.name, component.width, net.id, net.width
                        ),
                        component_source_with_net(document, component, &field, &net.id),
                        vec![net_source(document, net, "width")],
                        Some("Use identical widths on every V1 gate connection.".into()),
                    ));
                }
            }
        }

        if let Some((unknown, _)) = component.connections.iter().find(|(name, _)| {
            !definition
                .ports
                .iter()
                .any(|port| port.name == name.as_str())
        }) {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::ComponentUnknownConnection,
                DiagnosticSeverity::Error,
                format!(
                    "Component '{}' has unknown logical connection '{}'.",
                    component.id, unknown
                ),
                component_source(document, component, &format!("connections.{unknown}")),
                Vec::new(),
                Some(
                    "Remove the connection or use a logical port from the component catalog."
                        .into(),
                ),
            ));
        }

        for required in definition.required_parameters {
            if !component.parameters.contains_key(*required) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::ComponentMissingParameter,
                    DiagnosticSeverity::Error,
                    format!(
                        "Component '{}' is missing required parameter '{}'.",
                        component.id, required
                    ),
                    component_source(document, component, &format!("parameters.{required}")),
                    Vec::new(),
                    Some(format!("Provide the required '{required}' parameter.")),
                ));
            }
        }

        for parameter in component.parameters.keys() {
            if !definition.required_parameters.contains(&parameter.as_str()) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::ComponentUnknownParameter,
                    DiagnosticSeverity::Error,
                    format!(
                        "Component '{}' has unknown parameter '{}'.",
                        component.id, parameter
                    ),
                    component_source(document, component, &format!("parameters.{parameter}")),
                    Vec::new(),
                    Some("Remove parameters not declared by the V1 component catalog.".into()),
                ));
            }
        }

        if component.component_type == ComponentType::Const {
            validate_const(document, component, diagnostics);
        }
    }
}

fn validate_const(
    document: &CircuitDocument,
    component: &Component,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(value) = component.parameters.get("value") else {
        return;
    };
    let Some(literal) = value.as_str() else {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::ConstLiteralMalformed,
            DiagnosticSeverity::Error,
            format!(
                "CONST component '{}' value must be a binary string.",
                component.id
            ),
            component_source(document, component, "parameters.value"),
            Vec::new(),
            Some("Use a string containing only '0' and '1'.".into()),
        ));
        return;
    };
    if literal.is_empty() || !literal.bytes().all(|byte| matches!(byte, b'0' | b'1')) {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::ConstLiteralMalformed,
            DiagnosticSeverity::Error,
            format!(
                "CONST component '{}' value is not a non-empty binary string.",
                component.id
            ),
            component_source(document, component, "parameters.value"),
            Vec::new(),
            Some("Use exactly width ASCII characters, each '0' or '1'.".into()),
        ));
        return;
    }
    if literal.len() != component.width as usize {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::ConstValueWidthMismatch,
            DiagnosticSeverity::Error,
            format!(
                "CONST component '{}' has {} value bits but width {}.",
                component.id,
                literal.len(),
                component.width
            ),
            component_source(document, component, "parameters.value"),
            Vec::new(),
            Some(
                "Provide exactly one binary digit per output bit, including leading zeroes.".into(),
            ),
        ));
    }
}

#[derive(Debug, Clone, Default)]
struct NetRoles {
    drivers: Vec<Endpoint>,
    consumers: Vec<Endpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Endpoint {
    source: SourceReference,
    component_id: Option<String>,
}

fn build_electrical_roles<'a>(
    document: &CircuitDocument,
    index: &DocumentIndex<'a>,
) -> BTreeMap<&'a str, NetRoles> {
    let mut roles: BTreeMap<_, _> = index
        .unique_nets
        .keys()
        .map(|net_id| (*net_id, NetRoles::default()))
        .collect();

    for port in &document.circuit.ports {
        if index.duplicate_port_ids.contains(port.id.as_str()) {
            continue;
        }
        let Some(net_roles) = roles.get_mut(port.net_id.as_str()) else {
            continue;
        };
        let endpoint = Endpoint {
            source: port_source_with_net(document, port, "netId", &port.net_id),
            component_id: None,
        };
        match port.direction {
            PortDirection::Input => net_roles.drivers.push(endpoint),
            PortDirection::Output => net_roles.consumers.push(endpoint),
        }
    }

    for component in &document.circuit.components {
        if index
            .duplicate_component_ids
            .contains(component.id.as_str())
        {
            continue;
        }
        let Some(definition) = component_definition(component.component_type) else {
            continue;
        };
        for logical_port in definition.ports {
            let Some(net_id) = component.connections.get(logical_port.name) else {
                continue;
            };
            let Some(net_roles) = roles.get_mut(net_id.as_str()) else {
                continue;
            };
            let endpoint = Endpoint {
                source: component_source_with_net(
                    document,
                    component,
                    &format!("connections.{}", logical_port.name),
                    net_id,
                ),
                component_id: Some(component.id.clone()),
            };
            match logical_port.direction {
                PortDirection::Input => net_roles.consumers.push(endpoint),
                PortDirection::Output => net_roles.drivers.push(endpoint),
            }
        }
    }

    for net_roles in roles.values_mut() {
        net_roles.drivers.sort();
        net_roles.drivers.dedup();
        net_roles.consumers.sort();
        net_roles.consumers.dedup();
    }
    roles
}

fn validate_electrical_roles(
    document: &CircuitDocument,
    roles: &BTreeMap<&str, NetRoles>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (net_id, net_roles) in roles {
        let primary = source(document, None, Some(net_id), None, "connectivity");
        if net_roles.drivers.len() > 1 {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::NetMultipleDrivers,
                DiagnosticSeverity::Error,
                format!("Net '{net_id}' has {} drivers.", net_roles.drivers.len()),
                primary.clone(),
                net_roles
                    .drivers
                    .iter()
                    .map(|endpoint| endpoint.source.clone())
                    .collect(),
                Some("Ensure every V1 net has exactly one driver.".into()),
            ));
        }

        if net_roles.drivers.is_empty() && !net_roles.consumers.is_empty() {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::NetNoDriver,
                DiagnosticSeverity::Error,
                format!("Net '{net_id}' feeds consumers but has no driver."),
                primary.clone(),
                net_roles
                    .consumers
                    .iter()
                    .map(|endpoint| endpoint.source.clone())
                    .collect(),
                Some("Drive the net from an external input, component output, or CONST.".into()),
            ));
        }

        if net_roles.consumers.is_empty() {
            if net_roles.drivers.is_empty() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::NetUnused,
                    DiagnosticSeverity::Warning,
                    format!("Net '{net_id}' has no drivers or consumers."),
                    primary,
                    Vec::new(),
                    Some("Remove the unused declaration or connect it.".into()),
                ));
            } else {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::NetNoConsumers,
                    DiagnosticSeverity::Warning,
                    format!("Net '{net_id}' is driven but has no consumers."),
                    primary,
                    net_roles
                        .drivers
                        .iter()
                        .map(|endpoint| endpoint.source.clone())
                        .collect(),
                    Some("Connect the signal to a useful consumer or remove its driver.".into()),
                ));
            }
        }
    }
}

fn validate_cycles(
    document: &CircuitDocument,
    index: &DocumentIndex<'_>,
    roles: &BTreeMap<&str, NetRoles>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let nodes: BTreeSet<String> = document
        .circuit
        .components
        .iter()
        .filter(|component| component_definition(component.component_type).is_some())
        .map(|component| component.id.clone())
        .collect();
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = nodes
        .iter()
        .map(|node| (node.clone(), BTreeSet::new()))
        .collect();
    let mut edge_nets: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();

    for (net_id, net_roles) in roles {
        let drivers: BTreeSet<_> = net_roles
            .drivers
            .iter()
            .filter_map(|endpoint| endpoint.component_id.as_deref())
            .collect();
        let consumers: BTreeSet<_> = net_roles
            .consumers
            .iter()
            .filter_map(|endpoint| endpoint.component_id.as_deref())
            .collect();
        for driver in &drivers {
            for consumer in &consumers {
                if let Some(neighbors) = adjacency.get_mut(*driver) {
                    neighbors.insert((*consumer).to_owned());
                    edge_nets
                        .entry(((*driver).to_owned(), (*consumer).to_owned()))
                        .or_default()
                        .insert((*net_id).to_owned());
                }
            }
        }
    }

    let components = strongly_connected_components(&adjacency);
    for component_ids in components {
        let is_self_loop = component_ids.len() == 1
            && adjacency
                .get(&component_ids[0])
                .is_some_and(|neighbors| neighbors.contains(&component_ids[0]));
        if component_ids.len() == 1 && !is_self_loop {
            continue;
        }

        let members: BTreeSet<_> = component_ids.iter().cloned().collect();
        let mut net_ids = BTreeSet::new();
        for ((from, to), nets) in &edge_nets {
            if members.contains(from) && members.contains(to) {
                net_ids.extend(nets.iter().cloned());
            }
        }

        let primary_id = &component_ids[0];
        let mut related = component_ids
            .iter()
            .skip(1)
            .map(|id| source(document, Some(id), None, None, "cycle"))
            .collect::<Vec<_>>();
        related.extend(
            net_ids
                .iter()
                .map(|id| source(document, None, Some(id), None, "cycle")),
        );

        diagnostics.push(Diagnostic::new(
            DiagnosticCode::GraphCombinationalCycle,
            DiagnosticSeverity::Error,
            format!(
                "Combinational cycle contains components [{}] and nets [{}].",
                component_ids.join(", "),
                net_ids.into_iter().collect::<Vec<_>>().join(", ")
            ),
            source(document, Some(primary_id), None, None, "cycle"),
            related,
            Some(
                "Break the feedback path; V1 supports combinational acyclic circuits only.".into(),
            ),
        ));
    }

    let _ = index;
}

fn strongly_connected_components(
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<Vec<String>> {
    let mut visited = BTreeSet::new();
    let mut finish_order = Vec::with_capacity(adjacency.len());

    for start in adjacency.keys() {
        if visited.contains(start) {
            continue;
        }
        let mut stack = vec![(start.clone(), false)];
        while let Some((node, expanded)) = stack.pop() {
            if expanded {
                finish_order.push(node);
                continue;
            }
            if !visited.insert(node.clone()) {
                continue;
            }
            stack.push((node.clone(), true));
            if let Some(neighbors) = adjacency.get(&node) {
                for neighbor in neighbors.iter().rev() {
                    if !visited.contains(neighbor) {
                        stack.push((neighbor.clone(), false));
                    }
                }
            }
        }
    }

    let mut reverse: BTreeMap<String, BTreeSet<String>> = adjacency
        .keys()
        .map(|node| (node.clone(), BTreeSet::new()))
        .collect();
    for (from, neighbors) in adjacency {
        for to in neighbors {
            reverse.entry(to.clone()).or_default().insert(from.clone());
        }
    }

    let mut assigned = BTreeSet::new();
    let mut components = Vec::new();
    for start in finish_order.into_iter().rev() {
        if !assigned.insert(start.clone()) {
            continue;
        }
        let mut members = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            members.push(node.clone());
            if let Some(neighbors) = reverse.get(&node) {
                for neighbor in neighbors.iter().rev() {
                    if assigned.insert(neighbor.clone()) {
                        stack.push(neighbor.clone());
                    }
                }
            }
        }
        members.sort();
        components.push(members);
    }
    components.sort();
    components
}

fn circuit_source(document: &CircuitDocument, field: &str) -> SourceReference {
    source(document, None, None, None, field)
}

fn component_source(
    document: &CircuitDocument,
    component: &Component,
    field: &str,
) -> SourceReference {
    source(document, Some(&component.id), None, None, field)
}

fn component_source_with_net(
    document: &CircuitDocument,
    component: &Component,
    field: &str,
    net_id: &str,
) -> SourceReference {
    source(document, Some(&component.id), Some(net_id), None, field)
}

fn net_source(document: &CircuitDocument, net: &Net, field: &str) -> SourceReference {
    source(document, None, Some(&net.id), None, field)
}

fn port_source(document: &CircuitDocument, port: &ModulePort, field: &str) -> SourceReference {
    source(document, None, None, Some(&port.id), field)
}

fn port_source_with_net(
    document: &CircuitDocument,
    port: &ModulePort,
    field: &str,
    net_id: &str,
) -> SourceReference {
    source(document, None, Some(net_id), Some(&port.id), field)
}

fn source(
    document: &CircuitDocument,
    component_id: Option<&str>,
    net_id: Option<&str>,
    port_id: Option<&str>,
    field: &str,
) -> SourceReference {
    SourceReference {
        circuit_id: Some(document.circuit.id.clone()),
        component_id: component_id.map(str::to_owned),
        net_id: net_id.map(str::to_owned),
        port_id: port_id.map(str::to_owned),
        field: Some(field.to_owned()),
    }
}
