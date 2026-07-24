use crate::{ComponentType, PortDirection};

/// A logical port declared by a component catalog entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalPortDefinition {
    pub name: &'static str,
    pub direction: PortDirection,
}

/// The width relationship between ports on a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthRule {
    /// Every logical port has the component instance's `width`.
    Uniform,
}

/// The single authoritative definition of one V1 component type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentDefinition {
    pub component_type: ComponentType,
    pub ports: &'static [LogicalPortDefinition],
    pub input_arity: usize,
    pub width_rule: WidthRule,
    pub required_parameters: &'static [&'static str],
    pub output_semantics: &'static str,
}

const BINARY_PORTS: &[LogicalPortDefinition] = &[
    LogicalPortDefinition {
        name: "A",
        direction: PortDirection::Input,
    },
    LogicalPortDefinition {
        name: "B",
        direction: PortDirection::Input,
    },
    LogicalPortDefinition {
        name: "Y",
        direction: PortDirection::Output,
    },
];

const UNARY_PORTS: &[LogicalPortDefinition] = &[
    LogicalPortDefinition {
        name: "A",
        direction: PortDirection::Input,
    },
    LogicalPortDefinition {
        name: "Y",
        direction: PortDirection::Output,
    },
];

const OUTPUT_ONLY_PORTS: &[LogicalPortDefinition] = &[LogicalPortDefinition {
    name: "Y",
    direction: PortDirection::Output,
}];

/// The V1 component catalog. All catalog-aware code must use this table.
pub const COMPONENT_DEFINITIONS: &[ComponentDefinition] = &[
    ComponentDefinition {
        component_type: ComponentType::And,
        ports: BINARY_PORTS,
        input_arity: 2,
        width_rule: WidthRule::Uniform,
        required_parameters: &[],
        output_semantics: "Y = A & B",
    },
    ComponentDefinition {
        component_type: ComponentType::Or,
        ports: BINARY_PORTS,
        input_arity: 2,
        width_rule: WidthRule::Uniform,
        required_parameters: &[],
        output_semantics: "Y = A | B",
    },
    ComponentDefinition {
        component_type: ComponentType::Xor,
        ports: BINARY_PORTS,
        input_arity: 2,
        width_rule: WidthRule::Uniform,
        required_parameters: &[],
        output_semantics: "Y = A ^ B",
    },
    ComponentDefinition {
        component_type: ComponentType::Xnor,
        ports: BINARY_PORTS,
        input_arity: 2,
        width_rule: WidthRule::Uniform,
        required_parameters: &[],
        output_semantics: "Y = ~(A ^ B)",
    },
    ComponentDefinition {
        component_type: ComponentType::Nand,
        ports: BINARY_PORTS,
        input_arity: 2,
        width_rule: WidthRule::Uniform,
        required_parameters: &[],
        output_semantics: "Y = ~(A & B)",
    },
    ComponentDefinition {
        component_type: ComponentType::Nor,
        ports: BINARY_PORTS,
        input_arity: 2,
        width_rule: WidthRule::Uniform,
        required_parameters: &[],
        output_semantics: "Y = ~(A | B)",
    },
    ComponentDefinition {
        component_type: ComponentType::Not,
        ports: UNARY_PORTS,
        input_arity: 1,
        width_rule: WidthRule::Uniform,
        required_parameters: &[],
        output_semantics: "Y = ~A",
    },
    ComponentDefinition {
        component_type: ComponentType::Buffer,
        ports: UNARY_PORTS,
        input_arity: 1,
        width_rule: WidthRule::Uniform,
        required_parameters: &[],
        output_semantics: "Y = A",
    },
    ComponentDefinition {
        component_type: ComponentType::Const,
        ports: OUTPUT_ONLY_PORTS,
        input_arity: 0,
        width_rule: WidthRule::Uniform,
        required_parameters: &["value"],
        output_semantics: "Y is the exact-width binary string parameter 'value'",
    },
];

/// Finds the catalog definition for a V1 component type.
#[must_use]
pub fn component_definition(component_type: ComponentType) -> Option<&'static ComponentDefinition> {
    COMPONENT_DEFINITIONS
        .iter()
        .find(|definition| definition.component_type == component_type)
}
