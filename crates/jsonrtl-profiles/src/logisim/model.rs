//! Typed model of a Logisim / Logisim Evolution `.circ` project.
//!
//! `.circ` is XML. Only the parts that carry logic are modelled: the circuits,
//! their component instances, and the wire segments. Appearance, toolbar, and
//! option elements are ignored.

use std::collections::BTreeMap;
use std::path::Path;

use crate::ProfileError;

/// A point on the Logisim sheet. Connectivity is geometric, so coordinates are
/// identity: two things are connected exactly when they share a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

impl std::fmt::Display for Point {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "({},{})", self.x, self.y)
    }
}

/// One component instance inside a circuit.
#[derive(Debug, Clone, PartialEq)]
pub struct Comp {
    /// Library id. `None` means the component is a subcircuit of this project.
    pub lib: Option<String>,
    /// Component type name (`"AND Gate"`, `"Pin"`, or a subcircuit's name).
    pub name: String,
    pub loc: Point,
    pub attrs: BTreeMap<String, String>,
}

impl Comp {
    /// Attribute lookup.
    #[must_use]
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }

    /// Attribute parsed as an integer, when present and well formed.
    #[must_use]
    pub fn attr_int(&self, key: &str) -> Option<i64> {
        self.attr(key).and_then(|value| value.parse().ok())
    }

    /// True when the attribute is present and equal to `"true"`.
    #[must_use]
    pub fn attr_bool(&self, key: &str) -> bool {
        matches!(self.attr(key), Some("true"))
    }
}

/// A drawn wire segment. Segments join into nets wherever they share a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireSeg {
    pub from: Point,
    pub to: Point,
}

/// One `<circuit>` definition.
#[derive(Debug, Clone, PartialEq)]
pub struct CircuitDef {
    pub name: String,
    pub comps: Vec<Comp>,
    pub wires: Vec<WireSeg>,
}

/// A parsed `.circ` project.
#[derive(Debug, Clone, PartialEq)]
pub struct LogisimProject {
    /// The `source` attribute, e.g. `"2.7.1"` or `"3.8.0"` for Evolution.
    pub source: String,
    /// Name of the circuit marked `<main>`, when present.
    pub main: Option<String>,
    /// Circuit definition order as written, for stable output.
    pub circuit_names: Vec<String>,
    pub circuits: BTreeMap<String, CircuitDef>,
}

impl LogisimProject {
    /// True when the project was written by Logisim Evolution rather than the
    /// original Logisim. Evolution reports a major version of 3 or above.
    #[must_use]
    pub fn is_evolution(&self) -> bool {
        self.source
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
            .is_some_and(|major| major >= 3)
    }
}

/// Parses `(x,y)` as written in `loc`, `from`, and `to` attributes.
fn parse_point(raw: &str, context: &str) -> Result<Point, ProfileError> {
    let trimmed = raw.trim();
    let inner = trimmed
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .ok_or_else(|| ProfileError::Parse {
            path: context.to_string(),
            message: format!("expected a coordinate like '(10,20)', found '{raw}'"),
        })?;
    let (x, y) = inner.split_once(',').ok_or_else(|| ProfileError::Parse {
        path: context.to_string(),
        message: format!("coordinate '{raw}' is missing a comma"),
    })?;
    Ok(Point {
        x: x.trim().parse().map_err(|_| ProfileError::Parse {
            path: context.to_string(),
            message: format!("coordinate '{raw}' has a non-numeric x"),
        })?,
        y: y.trim().parse().map_err(|_| ProfileError::Parse {
            path: context.to_string(),
            message: format!("coordinate '{raw}' has a non-numeric y"),
        })?,
    })
}

/// Reads and parses a `.circ` file.
pub fn load_project(path: &Path) -> Result<LogisimProject, ProfileError> {
    let text = std::fs::read_to_string(path).map_err(|source| ProfileError::Io {
        path: path.display().to_string(),
        source,
    })?;
    parse_project(&text, &path.display().to_string())
}

/// Parses `.circ` XML already in memory.
pub fn parse_project(text: &str, context: &str) -> Result<LogisimProject, ProfileError> {
    let document = roxmltree::Document::parse(text).map_err(|error| ProfileError::Parse {
        path: context.to_string(),
        message: error.to_string(),
    })?;
    let root = document.root_element();
    if root.tag_name().name() != "project" {
        return Err(ProfileError::Parse {
            path: context.to_string(),
            message: format!(
                "expected a <project> root element, found <{}>",
                root.tag_name().name()
            ),
        });
    }

    let source = root.attribute("source").unwrap_or_default().to_string();
    let main = root
        .children()
        .find(|node| node.has_tag_name("main"))
        .and_then(|node| node.attribute("name"))
        .map(str::to_string);

    let mut circuit_names = Vec::new();
    let mut circuits = BTreeMap::new();
    for node in root.children().filter(|node| node.has_tag_name("circuit")) {
        let name = node
            .attribute("name")
            .ok_or_else(|| ProfileError::Parse {
                path: context.to_string(),
                message: "a <circuit> element has no name attribute".into(),
            })?
            .to_string();

        let mut comps = Vec::new();
        let mut wires = Vec::new();
        for child in node.children().filter(roxmltree::Node::is_element) {
            match child.tag_name().name() {
                "comp" => {
                    let loc = child.attribute("loc").ok_or_else(|| ProfileError::Parse {
                        path: context.to_string(),
                        message: format!("a <comp> in circuit '{name}' has no loc attribute"),
                    })?;
                    let mut attrs = BTreeMap::new();
                    for attribute in child.children().filter(|item| item.has_tag_name("a")) {
                        if let (Some(key), Some(value)) =
                            (attribute.attribute("name"), attribute.attribute("val"))
                        {
                            attrs.insert(key.to_string(), value.to_string());
                        }
                    }
                    comps.push(Comp {
                        lib: child.attribute("lib").map(str::to_string),
                        name: child.attribute("name").unwrap_or_default().to_string(),
                        loc: parse_point(loc, context)?,
                        attrs,
                    });
                }
                "wire" => {
                    let from = child.attribute("from").ok_or_else(|| ProfileError::Parse {
                        path: context.to_string(),
                        message: format!("a <wire> in circuit '{name}' has no from attribute"),
                    })?;
                    let to = child.attribute("to").ok_or_else(|| ProfileError::Parse {
                        path: context.to_string(),
                        message: format!("a <wire> in circuit '{name}' has no to attribute"),
                    })?;
                    wires.push(WireSeg {
                        from: parse_point(from, context)?,
                        to: parse_point(to, context)?,
                    });
                }
                _ => {}
            }
        }

        if circuits
            .insert(
                name.clone(),
                CircuitDef {
                    name: name.clone(),
                    comps,
                    wires,
                },
            )
            .is_some()
        {
            return Err(ProfileError::Structure {
                chip: name,
                detail: "circuit name is defined more than once".into(),
            });
        }
        circuit_names.push(name);
    }

    Ok(LogisimProject {
        source,
        main,
        circuit_names,
        circuits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<project source="3.8.0" version="1.0">
  <lib desc="#Wiring" name="0"/>
  <lib desc="#Gates" name="1"/>
  <main name="main"/>
  <circuit name="main">
    <a name="circuit" val="main"/>
    <comp lib="0" loc="(100,100)" name="Pin">
      <a name="label" val="A"/>
    </comp>
    <comp lib="1" loc="(200,100)" name="AND Gate">
      <a name="inputs" val="2"/>
    </comp>
    <wire from="(100,100)" to="(170,90)"/>
  </circuit>
</project>"##;

    #[test]
    fn parses_circuits_comps_and_wires() {
        let project = parse_project(SAMPLE, "sample").expect("parse");
        assert_eq!(project.main.as_deref(), Some("main"));
        assert!(project.is_evolution(), "3.8.0 is Evolution");
        let circuit = project.circuits.get("main").expect("main circuit");
        assert_eq!(circuit.comps.len(), 2);
        assert_eq!(circuit.wires.len(), 1);

        let pin = &circuit.comps[0];
        assert_eq!(pin.name, "Pin");
        assert_eq!(pin.loc, Point { x: 100, y: 100 });
        assert_eq!(pin.attr("label"), Some("A"));

        let gate = &circuit.comps[1];
        assert_eq!(gate.attr_int("inputs"), Some(2));
        assert_eq!(
            circuit.wires[0],
            WireSeg {
                from: Point { x: 100, y: 100 },
                to: Point { x: 170, y: 90 }
            }
        );
    }

    #[test]
    fn original_logisim_is_not_flagged_as_evolution() {
        let project = parse_project(&SAMPLE.replace("3.8.0", "2.7.1"), "sample").expect("parse");
        assert!(!project.is_evolution());
    }

    #[test]
    fn rejects_non_circ_xml() {
        let error = parse_project("<html><body/></html>", "sample").unwrap_err();
        assert!(matches!(error, ProfileError::Parse { .. }));
    }

    #[test]
    fn rejects_malformed_coordinates() {
        let broken = SAMPLE.replace(r##"loc="(100,100)""##, r##"loc="100,100""##);
        let error = parse_project(&broken, "sample").unwrap_err();
        match error {
            ProfileError::Parse { message, .. } => {
                assert!(message.contains("coordinate"), "{message}")
            }
            other => panic!("expected Parse, got {other:?}"),
        }
    }
}
