use std::collections::{BTreeMap, BTreeSet};

use crate::{
    CircuitDocument, ComponentType, PortDirection, SourceReference, VerilogIdentifier,
    component_definition,
};

/// Validated, deterministic compiler IR. It contains no UI metadata or raw names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedCircuit {
    original_id: String,
    identifier: VerilogIdentifier,
    ports: Vec<NormalizedPort>,
    components: Vec<NormalizedComponent>,
    nets: Vec<NormalizedNet>,
}

impl NormalizedCircuit {
    #[must_use]
    pub fn original_id(&self) -> &str {
        &self.original_id
    }

    #[must_use]
    pub const fn identifier(&self) -> &VerilogIdentifier {
        &self.identifier
    }

    #[must_use]
    pub fn ports(&self) -> &[NormalizedPort] {
        &self.ports
    }

    #[must_use]
    pub fn components(&self) -> &[NormalizedComponent] {
        &self.components
    }

    #[must_use]
    pub fn nets(&self) -> &[NormalizedNet] {
        &self.nets
    }
}

/// A normalized module port sorted by its original stable ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedPort {
    original_id: String,
    identifier: VerilogIdentifier,
    direction: PortDirection,
    width: u32,
    net_id: String,
    net_identifier: VerilogIdentifier,
}

impl NormalizedPort {
    #[must_use]
    pub fn original_id(&self) -> &str {
        &self.original_id
    }

    #[must_use]
    pub const fn identifier(&self) -> &VerilogIdentifier {
        &self.identifier
    }

    #[must_use]
    pub const fn direction(&self) -> PortDirection {
        self.direction
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn net_id(&self) -> &str {
        &self.net_id
    }

    #[must_use]
    pub const fn net_identifier(&self) -> &VerilogIdentifier {
        &self.net_identifier
    }
}

/// A normalized component sorted by its original stable ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedComponent {
    original_id: String,
    identifier: VerilogIdentifier,
    component_type: ComponentType,
    width: u32,
    connections: Vec<NormalizedConnection>,
    const_value: Option<String>,
}

impl NormalizedComponent {
    #[must_use]
    pub fn original_id(&self) -> &str {
        &self.original_id
    }

    #[must_use]
    pub const fn identifier(&self) -> &VerilogIdentifier {
        &self.identifier
    }

    #[must_use]
    pub const fn component_type(&self) -> ComponentType {
        self.component_type
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn connections(&self) -> &[NormalizedConnection] {
        &self.connections
    }

    #[must_use]
    pub fn connection(&self, logical_port: &str) -> Option<&NormalizedConnection> {
        self.connections
            .iter()
            .find(|connection| connection.logical_port == logical_port)
    }

    #[must_use]
    pub fn const_value(&self) -> Option<&str> {
        self.const_value.as_deref()
    }
}

/// A resolved component-port-to-net relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedConnection {
    logical_port: String,
    net_id: String,
    net_identifier: VerilogIdentifier,
}

impl NormalizedConnection {
    #[must_use]
    pub fn logical_port(&self) -> &str {
        &self.logical_port
    }

    #[must_use]
    pub fn net_id(&self) -> &str {
        &self.net_id
    }

    #[must_use]
    pub const fn net_identifier(&self) -> &VerilogIdentifier {
        &self.net_identifier
    }
}

/// A normalized internal net sorted by its original stable ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedNet {
    original_id: String,
    identifier: VerilogIdentifier,
    width: u32,
}

impl NormalizedNet {
    #[must_use]
    pub fn original_id(&self) -> &str {
        &self.original_id
    }

    #[must_use]
    pub const fn identifier(&self) -> &VerilogIdentifier {
        &self.identifier
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizationFailure {
    pub message: String,
    pub source: Box<SourceReference>,
}

pub(crate) fn normalize(
    document: &CircuitDocument,
) -> Result<NormalizedCircuit, NormalizationFailure> {
    let circuit = &document.circuit;
    let mut allocator = IdentifierAllocator::default();
    let identifier = allocator.allocate(&circuit.name);

    let mut public_ports: Vec<_> = circuit.ports.iter().collect();
    public_ports.sort_by(|left, right| left.id.cmp(&right.id));
    let port_identifiers: BTreeMap<_, _> = public_ports
        .iter()
        .map(|port| (port.id.as_str(), allocator.allocate(&port.name)))
        .collect();

    let mut public_nets: Vec<_> = circuit.nets.iter().collect();
    public_nets.sort_by(|left, right| left.id.cmp(&right.id));
    let net_identifiers: BTreeMap<_, _> = public_nets
        .iter()
        .map(|net| (net.id.as_str(), allocator.allocate(&net.name)))
        .collect();

    let mut public_components: Vec<_> = circuit.components.iter().collect();
    public_components.sort_by(|left, right| left.id.cmp(&right.id));
    let component_identifiers: BTreeMap<_, _> = public_components
        .iter()
        .map(|component| (component.id.as_str(), allocator.allocate(&component.name)))
        .collect();

    let mut ports = Vec::with_capacity(public_ports.len());
    for port in public_ports {
        let Some(port_identifier) = port_identifiers.get(port.id.as_str()) else {
            return Err(failure(document, None, None, Some(&port.id), "identifier"));
        };
        let Some(net_identifier) = net_identifiers.get(port.net_id.as_str()) else {
            return Err(failure(
                document,
                None,
                Some(&port.net_id),
                Some(&port.id),
                "netId",
            ));
        };
        ports.push(NormalizedPort {
            original_id: port.id.clone(),
            identifier: (*port_identifier).clone(),
            direction: port.direction,
            width: port.width,
            net_id: port.net_id.clone(),
            net_identifier: (*net_identifier).clone(),
        });
    }

    let nets = public_nets
        .into_iter()
        .map(|net| NormalizedNet {
            original_id: net.id.clone(),
            identifier: net_identifiers[net.id.as_str()].clone(),
            width: net.width,
        })
        .collect();

    let mut components = Vec::with_capacity(public_components.len());
    for component in public_components {
        let Some(definition) = component_definition(component.component_type) else {
            return Err(failure(document, Some(&component.id), None, None, "type"));
        };
        let Some(component_identifier) = component_identifiers.get(component.id.as_str()) else {
            return Err(failure(
                document,
                Some(&component.id),
                None,
                None,
                "identifier",
            ));
        };
        let mut connections = Vec::with_capacity(definition.ports.len());
        for logical_port in definition.ports {
            let Some(net_id) = component.connections.get(logical_port.name) else {
                return Err(failure(
                    document,
                    Some(&component.id),
                    None,
                    None,
                    &format!("connections.{}", logical_port.name),
                ));
            };
            let Some(net_identifier) = net_identifiers.get(net_id.as_str()) else {
                return Err(failure(
                    document,
                    Some(&component.id),
                    Some(net_id),
                    None,
                    &format!("connections.{}", logical_port.name),
                ));
            };
            connections.push(NormalizedConnection {
                logical_port: logical_port.name.to_owned(),
                net_id: net_id.clone(),
                net_identifier: (*net_identifier).clone(),
            });
        }
        let const_value = if component.component_type == ComponentType::Const {
            component
                .parameters
                .get("value")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        } else {
            None
        };
        if component.component_type == ComponentType::Const && const_value.is_none() {
            return Err(failure(
                document,
                Some(&component.id),
                None,
                None,
                "parameters.value",
            ));
        }
        components.push(NormalizedComponent {
            original_id: component.id.clone(),
            identifier: (*component_identifier).clone(),
            component_type: component.component_type,
            width: component.width,
            connections,
            const_value,
        });
    }

    Ok(NormalizedCircuit {
        original_id: circuit.id.clone(),
        identifier,
        ports,
        components,
        nets,
    })
}

#[derive(Debug, Default)]
struct IdentifierAllocator {
    used: BTreeSet<String>,
}

impl IdentifierAllocator {
    fn allocate(&mut self, raw: &str) -> VerilogIdentifier {
        let base = VerilogIdentifier::from_untrusted(raw).as_str().to_owned();
        if self.used.insert(base.clone()) {
            return VerilogIdentifier::from_safe(base);
        }

        let mut suffix = 2_u64;
        loop {
            let candidate = format!("{base}__{suffix}");
            if self.used.insert(candidate.clone()) {
                return VerilogIdentifier::from_safe(candidate);
            }
            suffix += 1;
        }
    }
}

fn failure(
    document: &CircuitDocument,
    component_id: Option<&str>,
    net_id: Option<&str>,
    port_id: Option<&str>,
    field: &str,
) -> NormalizationFailure {
    NormalizationFailure {
        message: format!("validated document violated normalization invariant at '{field}'"),
        source: Box::new(SourceReference {
            circuit_id: Some(document.circuit.id.clone()),
            component_id: component_id.map(str::to_owned),
            net_id: net_id.map(str::to_owned),
            port_id: port_id.map(str::to_owned),
            field: Some(field.to_owned()),
        }),
    }
}
