use serde::{Deserialize, Serialize};

/// A conservative simple Verilog-2001 identifier.
///
/// Construction sanitizes untrusted display text. The wrapped value is always
/// non-empty, starts with an ASCII letter or underscore, contains only ASCII
/// letters/digits/underscores, and is not a Verilog-2001 keyword.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct VerilogIdentifier(String);

impl VerilogIdentifier {
    /// Applies the kernel's single identifier policy to untrusted text.
    #[must_use]
    pub fn from_untrusted(value: &str) -> Self {
        Self(sanitize_identifier_candidate(value))
    }

    /// Returns the safe identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Confirms the conservative identifier invariants.
    #[must_use]
    pub fn is_safe(&self) -> bool {
        is_safe_identifier(&self.0)
    }

    pub(crate) fn from_safe(value: String) -> Self {
        debug_assert!(is_safe_identifier(&value));
        Self(value)
    }
}

impl std::fmt::Display for VerilogIdentifier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for VerilogIdentifier {
    fn deserialize<Deserializer>(deserializer: Deserializer) -> Result<Self, Deserializer::Error>
    where
        Deserializer: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if is_safe_identifier(&value) {
            Ok(Self(value))
        } else {
            Err(serde::de::Error::custom(
                "value is not a safe simple Verilog-2001 identifier",
            ))
        }
    }
}

pub(crate) fn sanitize_identifier_candidate(value: &str) -> String {
    let mut sanitized = sanitized_stem(value);
    if is_verilog_keyword(&sanitized) {
        sanitized.push_str("_id");
    }
    sanitized
}

pub(crate) fn sanitized_stem(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len().saturating_add(2));
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            sanitized.push(character);
        } else {
            sanitized.push('_');
        }
    }
    if sanitized.is_empty() {
        sanitized.push_str("unnamed");
    }
    if sanitized.as_bytes()[0].is_ascii_digit() {
        sanitized.insert_str(0, "n_");
    }
    sanitized
}

pub(crate) fn is_safe_identifier(value: &str) -> bool {
    let Some(first) = value.as_bytes().first() else {
        return false;
    };
    (first.is_ascii_alphabetic() || *first == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && !is_verilog_keyword(value)
}

pub(crate) fn is_verilog_keyword(name: &str) -> bool {
    matches!(
        name,
        "always"
            | "and"
            | "assign"
            | "automatic"
            | "begin"
            | "buf"
            | "bufif0"
            | "bufif1"
            | "case"
            | "casex"
            | "casez"
            | "cell"
            | "cmos"
            | "config"
            | "deassign"
            | "default"
            | "defparam"
            | "design"
            | "disable"
            | "edge"
            | "else"
            | "end"
            | "endcase"
            | "endconfig"
            | "endfunction"
            | "endgenerate"
            | "endmodule"
            | "endprimitive"
            | "endspecify"
            | "endtable"
            | "endtask"
            | "event"
            | "for"
            | "force"
            | "forever"
            | "fork"
            | "function"
            | "generate"
            | "genvar"
            | "highz0"
            | "highz1"
            | "if"
            | "ifnone"
            | "incdir"
            | "include"
            | "initial"
            | "inout"
            | "input"
            | "instance"
            | "integer"
            | "join"
            | "large"
            | "liblist"
            | "library"
            | "localparam"
            | "macromodule"
            | "medium"
            | "module"
            | "nand"
            | "negedge"
            | "nmos"
            | "nor"
            | "noshowcancelled"
            | "not"
            | "notif0"
            | "notif1"
            | "or"
            | "output"
            | "parameter"
            | "pmos"
            | "posedge"
            | "primitive"
            | "pull0"
            | "pull1"
            | "pulldown"
            | "pullup"
            | "pulsestyle_onevent"
            | "pulsestyle_ondetect"
            | "rcmos"
            | "real"
            | "realtime"
            | "reg"
            | "release"
            | "repeat"
            | "rnmos"
            | "rpmos"
            | "rtran"
            | "rtranif0"
            | "rtranif1"
            | "scalared"
            | "showcancelled"
            | "signed"
            | "small"
            | "specify"
            | "specparam"
            | "strong0"
            | "strong1"
            | "supply0"
            | "supply1"
            | "table"
            | "task"
            | "time"
            | "tran"
            | "tranif0"
            | "tranif1"
            | "tri"
            | "tri0"
            | "tri1"
            | "triand"
            | "trior"
            | "trireg"
            | "unsigned"
            | "use"
            | "vectored"
            | "wait"
            | "wand"
            | "weak0"
            | "weak1"
            | "while"
            | "wire"
            | "wor"
            | "xnor"
            | "xor"
    )
}
