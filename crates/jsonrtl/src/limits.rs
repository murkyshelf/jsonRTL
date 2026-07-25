/// Resource limits applied before semantic graph validation.
///
/// The JSON Schema enforces the public V1 maxima. A `KernelLimits` value can make a
/// deployment stricter without changing document meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelLimits {
    pub max_document_bytes: usize,
    pub max_ports: usize,
    pub max_components: usize,
    pub max_nets: usize,
    pub max_width: u32,
    pub max_string_length: usize,
    pub max_parameters_per_component: usize,
}

impl Default for KernelLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: 1_048_576,
            max_ports: 256,
            max_components: 10_000,
            max_nets: 20_000,
            max_width: 4_096,
            max_string_length: 128,
            max_parameters_per_component: 32,
        }
    }
}
