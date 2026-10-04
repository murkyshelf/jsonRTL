use std::collections::BTreeSet;

use jsonrtl_profiles::{ProfileError, dls::export::from_yosys_json};
use serde_json::{Value, json};

fn design(ports: Value, cells: Value) -> Value {
    json!({"modules":{"top":{"attributes":{},"ports":ports,"cells":cells,"netnames":{}}}})
}

fn port(direction: &str, bits: Value) -> Value {
    json!({"direction":direction,"bits":bits})
}

fn cell(kind: &str, connections: Value) -> Value {
    let directions = connections
        .as_object()
        .unwrap()
        .keys()
        .map(|name| {
            (
                name.clone(),
                json!(if name == "Y" || name == "Q" {
                    "output"
                } else {
                    "input"
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({"type":kind,"parameters":{},"attributes":{},"port_directions":directions,"connections":connections})
}

fn export(value: &Value) -> jsonrtl_profiles::dls::export::ExportProject {
    from_yosys_json(&value.to_string(), "top", "test project", None).unwrap()
}

fn chip(project: &jsonrtl_profiles::dls::export::ExportProject, name: &str) -> Value {
    serde_json::from_str(&project.chips[name]).unwrap()
}

#[test]
fn exports_real_dls_file_shape_and_unique_addresses() {
    let project = export(&design(
        json!({"a":port("input",json!([2])),"b":port("input",json!([3])),"y":port("output",json!([4]))}),
        json!({"gate":cell("$_NAND_",json!({"A":[2],"B":[3],"Y":[4]}))}),
    ));
    let description: Value = serde_json::from_str(&project.project_description).unwrap();
    assert_eq!(description["ProjectName"], "test project");
    assert_eq!(description["DLSVersion_LastSaved"], "2.1.6");
    assert!(description["StarredList"].is_array());
    assert!(description["ChipCollections"].is_array());
    assert_eq!(description["Prefs_SimPaused"], false);
    assert_eq!(description["Prefs_SimTargetStepsPerSecond"], 1000);
    assert_eq!(description["Prefs_SimStepsPerClockTick"], 250);
    for name in project.chips.keys() {
        let c = chip(&project, name);
        assert_eq!(c["Name"], *name);
        assert_eq!(c["DLSVersion"], "2.1.6");
        assert_eq!(c["ChipType"], 0);
        assert!(c["Size"]["x"].as_f64().unwrap() > 0.0);
        assert!(c["Colour"]["a"].is_number());
        assert!(c["Displays"].is_array());
        let mut ids = BTreeSet::new();
        for field in ["InputPins", "OutputPins", "SubChips"] {
            for item in c[field].as_array().unwrap() {
                let id = item["ID"].as_i64().unwrap();
                assert!(
                    id > 0 && ids.insert(id),
                    "duplicate/nonpositive ID in {name}"
                );
                assert!(item["Position"]["x"].is_number());
                if field != "SubChips" {
                    assert!([1, 4, 8].contains(&item["BitCount"].as_u64().unwrap()));
                    assert!(item["Colour"].is_number());
                    assert!(item["ValueDisplayMode"].is_number());
                } else {
                    assert!(item.get("Label").is_some());
                    assert!(item.get("InternalData").is_some());
                    assert!(item["OutputPinColourInfo"].is_array());
                }
            }
        }
        for wire in c["Wires"].as_array().unwrap() {
            assert_eq!(wire["ConnectionType"], 0);
            assert_eq!(wire["ConnectedWireIndex"], -1);
            assert_eq!(wire["ConnectedWireSegmentIndex"], -1);
            assert!(wire["Points"].as_array().unwrap().len() >= 2);
            for address in ["SourcePinAddress", "TargetPinAddress"] {
                assert!(ids.contains(&wire[address]["PinOwnerID"].as_i64().unwrap()));
            }
        }
    }
    let c = chip(&project, &project.top_chip);
    assert_eq!(c["SubChips"][0]["Name"], "NAND");
    assert_eq!(c["Wires"].as_array().unwrap().len(), 3);
}

#[test]
fn wide_ports_use_actual_pin_widths_and_msb_first_converters() {
    let bits: Vec<u32> = (2..17).collect();
    let project = export(&design(
        json!({"a":port("input",json!(bits)),"y":port("output",json!(bits))}),
        json!({}),
    ));
    let c = chip(&project, &project.top_chip);
    let widths: Vec<_> = c["InputPins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["BitCount"].as_u64().unwrap())
        .collect();
    assert_eq!(widths, [8, 4, 1, 1, 1]);
    assert_eq!(c["InputPins"][0]["Name"], "a[7:0]");
    assert_eq!(c["InputPins"][1]["Name"], "a[11:8]");
    assert_eq!(c["OutputPins"][0]["ValueDisplayMode"], 1);
    assert_eq!(c["OutputPins"][2]["ValueDisplayMode"], 0);
    let split = c["SubChips"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["Name"] == "8-1BIT")
        .unwrap();
    let merge = c["SubChips"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["Name"] == "1-8BIT")
        .unwrap();
    for i in 0..8 {
        assert!(
            c["Wires"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["SourcePinAddress"]["PinOwnerID"] == split["ID"]
                    && w["SourcePinAddress"]["PinID"] == i + 1
                    && w["TargetPinAddress"]["PinOwnerID"] == merge["ID"]
                    && w["TargetPinAddress"]["PinID"] == i)
        );
    }
}

#[test]
fn vector_labels_respect_yosys_offset_and_upto() {
    let mut a = port("input", json!([2, 3, 4, 5, 6]));
    a["offset"] = json!(10);
    a["upto"] = json!(1);
    let project = export(&design(
        json!({"a":a,"y":port("output",json!([2]))}),
        json!({}),
    ));
    let c = chip(&project, &project.top_chip);
    assert_eq!(c["InputPins"][0]["Name"], "a[11:14]");
    assert_eq!(c["InputPins"][1]["Name"], "a[10]");
}

#[test]
fn constants_aliases_and_unused_inputs_are_preserved() {
    let project = export(&design(
        json!({"unused":port("input",json!([2])),"one":port("output",json!(["1"])),"zero":port("output",json!(["0"])),"one_alias":port("output",json!(["1"]))}),
        json!({}),
    ));
    let c = chip(&project, &project.top_chip);
    assert_eq!(c["InputPins"].as_array().unwrap().len(), 1);
    assert_eq!(c["OutputPins"].as_array().unwrap().len(), 3);
    assert!(project.chips.contains_key("JSONRTL_CONST1"));
    assert!(project.chips.contains_key("JSONRTL_CONST0"));
    assert!(
        chip(&project, "JSONRTL_CONST1")["InputPins"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn edge_registers_and_reset_variants_have_custom_nand_helpers() {
    for kind in [
        "$_DFF_P_",
        "$_DFF_N_",
        "$_DFF_PP0_",
        "$_DFF_PP1_",
        "$_DFF_PN0_",
        "$_DFF_PN1_",
        "$_DFF_NP0_",
        "$_DFF_NP1_",
        "$_DFF_NN0_",
        "$_DFF_NN1_",
        "$_DLATCH_P_",
        "$_DLATCH_N_",
    ] {
        let mut connections = json!({"D":[2],"Q":[4]});
        connections[if kind.contains("DLATCH") { "E" } else { "C" }] = json!([3]);
        if kind.len() == 10 {
            connections["R"] = json!([5]);
        }
        let project = export(&design(
            json!({"d":port("input",json!([2])),"c":port("input",json!([3])),"r":port("input",json!([5])),"q":port("output",json!([4]))}),
            json!({"ff":cell(kind,connections)}),
        ));
        let c = chip(&project, &project.top_chip);
        let helper = c["SubChips"][0]["Name"].as_str().unwrap();
        assert!(helper.starts_with("JSONRTL_"), "{kind} generated {helper}");
        assert!(project.chips.contains_key(helper));
        for definition in project.chips.keys() {
            for sub in chip(&project, definition)["SubChips"].as_array().unwrap() {
                assert!(
                    sub["Name"] == "NAND"
                        || project.chips.contains_key(sub["Name"].as_str().unwrap())
                );
            }
        }
    }
}

#[test]
fn sequential_feedback_is_allowed_and_combinational_feedback_is_rejected() {
    let ports = json!({"clk":port("input",json!([2])),"q":port("output",json!([3]))});
    let ff = cell("$_DFF_P_", json!({"C":[2],"D":[4],"Q":[3]}));
    let inv = cell("$_NOT_", json!({"A":[3],"Y":[4]}));
    export(&design(ports, json!({"ff":ff,"inv":inv})));
    let error = from_yosys_json(&design(json!({"q":port("output",json!([3]))}),json!({"a":cell("$_NOT_",json!({"A":[4],"Y":[3]})),"b":cell("$_NOT_",json!({"A":[3],"Y":[4]}))})).to_string(),"top","p",None).unwrap_err();
    assert!(error.to_string().contains("combinational cycle"), "{error}");
}

#[test]
fn clock_wrapper_uses_clock_builtin_and_exposes_other_ports() {
    let source = design(
        json!({"clk":port("input",json!([2])),"data":port("input",json!([3])),"q":port("output",json!([4]))}),
        json!({"ff":cell("$_DFF_P_",json!({"C":[2],"D":[3],"Q":[4]}))}),
    );
    let project = from_yosys_json(&source.to_string(), "top", "p", Some("clk")).unwrap();
    let c = chip(&project, &project.top_chip);
    assert_eq!(c["InputPins"].as_array().unwrap().len(), 1);
    assert_eq!(c["InputPins"][0]["Name"], "data");
    assert!(
        c["SubChips"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["Name"] == "CLOCK")
    );
    for invalid in ["absent", "q", "data_bus"] {
        assert!(from_yosys_json(&source.to_string(), "top", "p", Some(invalid)).is_err());
    }
}

#[test]
fn name_sanitization_avoids_paths_and_case_insensitive_builtin_or_helper_collisions() {
    for name in [
        "NAND",
        "nand",
        "JSONRTL_CONST1",
        "../escaped",
        "top/part",
        "",
    ] {
        let source = json!({"modules":{name:{"attributes":{},"ports":{"y":port("output",json!(["1"]))},"cells":{},"netnames":{}}}});
        let project = from_yosys_json(&source.to_string(), name, "p", None).unwrap();
        assert!(jsonrtl_profiles::is_safe_unit_name(&project.top_chip));
        assert_ne!(project.top_chip.to_ascii_lowercase(), "nand");
        let names: BTreeSet<_> = project
            .chips
            .keys()
            .map(|n| n.to_ascii_lowercase())
            .collect();
        assert_eq!(names.len(), project.chips.len());
    }
}

#[test]
fn output_is_deterministic() {
    let source = design(json!({"y":port("output",json!(["0"]))}), json!({}));
    let a = export(&source);
    let b = export(&source);
    assert_eq!(a.top_chip, b.top_chip);
    assert_eq!(a.chips, b.chips);
    assert_eq!(a.project_description, b.project_description);
}

#[test]
fn unsupported_cells_unknown_states_and_undriven_or_duplicate_bits_fail_explicitly() {
    let examples = [
        (
            design(json!({"y":port("output",json!(["x"]))}), json!({})),
            "x",
        ),
        (
            design(json!({"y":port("output",json!(["z"]))}), json!({})),
            "z",
        ),
        (
            design(json!({"y":port("output",json!([2]))}), json!({})),
            "undriven",
        ),
        (
            design(
                json!({"a":port("input",json!([2])),"b":port("input",json!([2]))}),
                json!({}),
            ),
            "multiple drivers",
        ),
        (
            design(
                json!({}),
                json!({"t":cell("$_TBUF_",json!({"A":["0"],"E":["1"],"Y":[2]}))}),
            ),
            "$_TBUF_",
        ),
    ];
    for (source, reason) in examples {
        let error = from_yosys_json(&source.to_string(), "top", "p", None).unwrap_err();
        assert!(
            error.to_string().contains(reason),
            "expected {reason}: {error}"
        );
    }
}

#[test]
fn malformed_and_non_flattened_yosys_json_are_rejected() {
    for source in [
        json!({}),
        json!({"modules":{}}),
        design(json!({"a":{}}), json!({})),
        design(json!({"a":port("inout",json!([2]))}), json!({})),
        design(json!({"a":port("input",json!([]))}), json!({})),
        design(
            json!({}),
            json!({"n":cell("$_NOT_",json!({"A":["0","1"],"Y":[2]}))}),
        ),
    ] {
        assert!(
            from_yosys_json(&source.to_string(), "top", "p", None).is_err(),
            "accepted {source}"
        );
    }
    let mut source = design(json!({}), json!({}));
    source["modules"]["top"]["attributes"]["blackbox"] = json!("1");
    assert!(
        from_yosys_json(&source.to_string(), "top", "p", None)
            .unwrap_err()
            .to_string()
            .contains("blackbox")
    );
    source["modules"]["top"]["attributes"] = json!({});
    source["modules"]["top"]["netnames"] = json!({"q":{"bits":[2],"attributes":{"init":"0"}}});
    assert!(
        from_yosys_json(&source.to_string(), "top", "p", None)
            .unwrap_err()
            .to_string()
            .contains("init")
    );
    source["modules"]["top"]["netnames"] = json!({});
    source["modules"]["top"]["memories"] = json!({"ram":{}});
    assert!(
        from_yosys_json(&source.to_string(), "top", "p", None)
            .unwrap_err()
            .to_string()
            .contains("memor")
    );
}

#[test]
fn rejects_resource_limits_before_generation() {
    let huge = " ".repeat(32 * 1024 * 1024 + 1);
    assert!(matches!(
        from_yosys_json(&huge, "top", "p", None),
        Err(ProfileError::Limit { .. })
    ));
    let source = design(
        json!({"a":port("input",json!((2..4099).collect::<Vec<u32>>()))}),
        json!({}),
    );
    assert!(matches!(
        from_yosys_json(&source.to_string(), "top", "p", None),
        Err(ProfileError::Limit { .. })
    ));
    let cells = (0..10001)
        .map(|i| {
            (
                format!("n{i}"),
                cell("$_NOT_", json!({"A":["0"],"Y":[i+2]})),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let source = design(json!({}), json!(cells));
    assert!(matches!(
        from_yosys_json(&source.to_string(), "top", "p", None),
        Err(ProfileError::Limit { .. })
    ));
}

#[test]
fn duplicate_json_map_keys_are_rejected_instead_of_discarding_logic() {
    for source in [
        r#"{"modules":{"top":{"ports":{},"cells":{},"ports":{}}}}"#,
        r#"{"modules":{"top":{"ports":{},"cells":{}},"top":{"ports":{},"cells":{}}}}"#,
        r#"{"modules":{"top":{"ports":{"a":{"direction":"input","bits":[2]},"a":{"direction":"input","bits":[3]}},"cells":{}}}}"#,
        r#"{"modules":{"top":{"ports":{},"cells":{"g":{"type":"$_NOT_","parameters":{},"port_directions":{"A":"input","Y":"output"},"connections":{"A":["0"],"Y":[2]}},"g":{"type":"$_NOT_","parameters":{},"port_directions":{"A":"input","Y":"output"},"connections":{"A":["1"],"Y":[3]}}}}}}"#,
    ] {
        assert!(
            matches!(
                from_yosys_json(source, "top", "p", None),
                Err(ProfileError::Parse { .. })
            ),
            "accepted duplicate key: {source}"
        );
    }
}

#[test]
fn generated_chip_size_limit_is_enforced() {
    let name = "p".repeat(8 * 1024 * 1024 + 1);
    let source = design(json!({name:port("input",json!([2]))}), json!({}));
    assert!(matches!(
        from_yosys_json(&source.to_string(), "top", "p", None),
        Err(ProfileError::Limit { .. })
    ));
}
