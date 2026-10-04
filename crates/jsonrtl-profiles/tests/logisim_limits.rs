use std::sync::atomic::{AtomicU64, Ordering};

use jsonrtl_profiles::{
    Profile, ProfileError,
    logisim::{LogisimProfile, elaborate, model},
};

fn convert_xml(xml: &str) -> Result<elaborate::FlatNetlist, ProfileError> {
    let project = model::parse_project(xml, "hostile.circ")?;
    elaborate::elaborate(&project, "main")
}

fn gate(attributes: &str, location: &str) -> String {
    format!(
        r#"<project source="3.8.0"><circuit name="main"><comp lib="1" name="AND Gate" loc="{location}">{attributes}</comp></circuit></project>"#
    )
}

struct ProjectFile(std::path::PathBuf);

impl ProjectFile {
    fn new(xml: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "jsonrtl-logisim-siblings-{}-{}.circ",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, xml).expect("write project");
        Self(path)
    }
}

impl Drop for ProjectFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn sibling_project(bad_body: &str) -> String {
    format!(
        r#"<project><circuit name="good">
            <comp lib="0" name="Pin" loc="(0,0)"><a name="label" val="in"/></comp>
            <comp lib="0" name="Pin" loc="(0,0)"><a name="label" val="out"/><a name="output" val="true"/></comp>
        </circuit><circuit name="bad">{bad_body}</circuit></project>"#
    )
}

fn bad_sibling_cases() -> [(&'static str, bool); 7] {
    [
        (
            r#"<comp lib="1" name="AND Gate" loc="(100,100)"><a name="inputs" val="65"/></comp>"#,
            true,
        ),
        (
            r#"<comp lib="1" name="AND Gate" loc="(-9223372036854775808,100)"/>"#,
            true,
        ),
        (r#"<wire from="(0,0)" to="(9223372036854775807,0)"/>"#, true),
        (
            r#"<comp lib="1" name="AND Gate" loc="(100,100)"><a name="size" val="1001"/></comp>"#,
            true,
        ),
        (
            r#"<comp lib="1" name="AND Gate" loc="(100,100)"><a name="inputs" val="many"/></comp>"#,
            false,
        ),
        (
            r#"<comp lib="1" name="AND Gate" loc="(100,100)"><a name="inputs"/></comp>"#,
            false,
        ),
        (
            r#"<comp lib="1" name="AND Gate" loc="(100,100)"><a name="facing" val="sideways"/></comp>"#,
            false,
        ),
    ]
}

#[test]
fn selected_good_circuit_is_not_blocked_by_sibling_geometry_errors() {
    for (bad, _) in bad_sibling_cases() {
        let file = ProjectFile::new(&sibling_project(bad));
        let conversion = LogisimProfile
            .convert_unit(&file.0, "good")
            .expect("good sibling converts");
        assert_eq!(conversion.circuits.len(), 1);
        assert_eq!(conversion.circuits[0].name, "good");
        let result = jsonrtl::Kernel::default().compile_verilog(
            &conversion.circuits[0].document,
            &jsonrtl::CompileOptions::default(),
        );
        assert!(result.has_output(), "{bad}: {:?}", result.diagnostics);
    }
}

#[test]
fn batch_conversion_keeps_geometry_errors_local_to_the_bad_circuit() {
    for (bad, exceeds_limit) in bad_sibling_cases() {
        let file = ProjectFile::new(&sibling_project(bad));
        let units = LogisimProfile
            .units(&file.0)
            .expect("geometry does not block listing");
        assert_eq!(units.unit_names, ["good", "bad"]);
        let mut results = LogisimProfile
            .convert_units(&file.0, &["good".into(), "bad".into()])
            .expect("batch project loads")
            .into_iter();
        assert_eq!(
            results
                .next()
                .unwrap()
                .expect("good circuit converts")
                .circuits[0]
                .name,
            "good"
        );
        let error = results
            .next()
            .unwrap()
            .expect_err("bad circuit is rejected");
        if exceeds_limit {
            assert!(
                matches!(error, ProfileError::Limit { .. }),
                "{bad}: {error:?}"
            );
        } else {
            assert!(
                matches!(error, ProfileError::Parse { .. }),
                "{bad}: {error:?}"
            );
        }
        assert!(results.next().is_none());
    }
}

#[test]
fn huge_gate_input_count_returns_a_limit_error() {
    let xml = gate(
        r#"<a name="inputs" val="9223372036854775807"/>"#,
        "(100,100)",
    );
    assert!(matches!(convert_xml(&xml), Err(ProfileError::Limit { .. })));
}

#[test]
fn extreme_coordinates_and_sizes_return_errors_without_overflowing() {
    for xml in [
        gate("", "(-9223372036854775808,100)"),
        gate("", "(100,9223372036854775807)"),
        gate(
            r#"<a name="size" val="9223372036854775807"/><a name="facing" val="west"/>"#,
            "(100,100)",
        ),
    ] {
        assert!(
            matches!(convert_xml(&xml), Err(ProfileError::Limit { .. })),
            "{xml}"
        );
    }
}

#[test]
fn malformed_geometry_attributes_are_not_replaced_with_defaults() {
    for attributes in [
        r#"<a name="inputs" val="many"/>"#,
        r#"<a name="inputs" val="0"/>"#,
        r#"<a name="inputs" val="-2"/>"#,
        r#"<a name="size" val="large"/>"#,
        r#"<a name="size" val="0"/>"#,
        r#"<a name="size" val="-30"/>"#,
        r#"<a name="width" val="one"/>"#,
        r#"<a name="facing" val="sideways"/>"#,
        r#"<a name="inputs"/>"#,
        r#"<a name="size"/>"#,
        r#"<a name="width"/>"#,
        r#"<a name="facing"/>"#,
    ] {
        assert!(
            matches!(
                convert_xml(&gate(attributes, "(100,100)")),
                Err(ProfileError::Parse { .. })
            ),
            "{attributes}"
        );
    }
}

#[test]
fn limit_adjacent_geometry_keeps_all_inputs_and_facing() {
    let xml = gate(
        r#"<a name="inputs" val="64"/><a name="size" val="1000"/><a name="facing" val="west"/>"#,
        "(1000000,-1000000)",
    );
    let flat = convert_xml(&xml).expect("bounded geometry should convert");
    assert_eq!(flat.gates[0].inputs.len(), 64);
    assert_eq!(flat.net_count, 65);
}

#[test]
fn xml_over_the_byte_limit_is_rejected_before_parsing() {
    let xml = format!("<project/>{}", " ".repeat(8 * 1024 * 1024));
    assert!(matches!(
        model::parse_project(&xml, "big.circ"),
        Err(ProfileError::Limit { .. })
    ));
}

#[test]
fn oversized_files_are_rejected_before_reading_or_parsing() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "jsonrtl-logisim-{}-{}.circ",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let file = std::fs::File::create(&path).expect("create fixture");
    file.set_len(8 * 1024 * 1024 + 1).expect("resize fixture");
    let result = LogisimProfile.units(&path);
    std::fs::remove_file(path).expect("remove fixture");
    assert!(matches!(result, Err(ProfileError::Limit { .. })));
}

#[test]
fn too_many_circuits_are_rejected() {
    let mut xml = String::from("<project>");
    for index in 0..257 {
        xml.push_str(&format!(r#"<circuit name="c{index}"/>"#));
    }
    xml.push_str("</project>");
    assert!(matches!(
        model::parse_project(&xml, "many.circ"),
        Err(ProfileError::Limit { .. })
    ));
}

#[test]
fn too_many_components_are_rejected_before_elaboration() {
    let xml = format!(
        r#"<project><circuit name="main">{}</circuit></project>"#,
        r#"<comp lib="0" name="Pin" loc="(0,0)"/>"#.repeat(20_001)
    );
    assert!(matches!(
        model::parse_project(&xml, "many.circ"),
        Err(ProfileError::Limit { .. })
    ));
}

#[test]
fn too_many_wires_are_rejected_before_connectivity_work() {
    let xml = format!(
        r#"<project><circuit name="main">{}</circuit></project>"#,
        r#"<wire from="(0,0)" to="(10,0)"/>"#.repeat(20_001)
    );
    assert!(matches!(
        model::parse_project(&xml, "many.circ"),
        Err(ProfileError::Limit { .. })
    ));
}

#[test]
fn project_counts_are_bounded_across_circuits() {
    let pins = r#"<comp lib="0" name="Pin" loc="(0,0)"/>"#.repeat(10_001);
    let xml = format!(
        r#"<project><circuit name="one">{pins}</circuit><circuit name="two">{pins}</circuit></project>"#
    );
    assert!(matches!(
        model::parse_project(&xml, "many.circ"),
        Err(ProfileError::Limit { .. })
    ));
}

#[test]
fn ignored_xml_elements_cannot_bypass_the_node_budget() {
    let xml = format!("<project>{}</project>", "<ignored/>".repeat(100_000));
    assert!(matches!(
        model::parse_project(&xml, "many.circ"),
        Err(ProfileError::Limit { .. })
    ));
}

#[test]
fn the_total_port_budget_limits_geometry_allocation() {
    let xml = format!(
        r#"<project><circuit name="main">{}</circuit></project>"#,
        r#"<comp lib="1" name="AND Gate" loc="(0,0)"><a name="inputs" val="64"/></comp>"#
            .repeat(1_539)
    );
    assert!(matches!(convert_xml(&xml), Err(ProfileError::Limit { .. })));
}

#[test]
fn quadratic_connectivity_work_is_bounded_before_geometry_is_allocated() {
    let xml = format!(
        r#"<project><circuit name="main">{}</circuit></project>"#,
        r#"<wire from="(0,0)" to="(10,0)"/>"#.repeat(2_237)
    );
    assert!(matches!(convert_xml(&xml), Err(ProfileError::Limit { .. })));
}
