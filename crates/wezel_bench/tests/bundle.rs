use serde_json::json;
use wezel_bench::build_bundle;
use wezel_types::ForagerSchema;

fn fake_forager(name: &str, doc: &str, properties: serde_json::Value) -> ForagerSchema {
    ForagerSchema {
        name: name.into(),
        description: format!("fake {name}"),
        inputs: json!({
            "type": "object",
            "properties": properties,
            "required": ["cmd"],
        }),
        measurements_doc: doc.into(),
    }
}

#[test]
fn bundle_has_one_branch_per_forager() {
    let bundle = build_bundle([
        fake_forager(
            "exec",
            "no measurements",
            json!({ "cmd": { "type": "string" } }),
        ),
        fake_forager(
            "cargo",
            "time_ms",
            json!({ "command": { "type": "string" } }),
        ),
    ]);

    let branches = bundle
        .pointer("/definitions/ExperimentStepToml/allOf")
        .and_then(|v| v.as_array())
        .expect("allOf branches exist");
    assert_eq!(branches.len(), 2);
    let tool_consts: Vec<&str> = branches
        .iter()
        .map(|b| {
            b.pointer("/if/properties/tool/const")
                .unwrap()
                .as_str()
                .unwrap()
        })
        .collect();
    assert_eq!(tool_consts, ["exec", "cargo"]);
}

#[test]
fn branch_splices_forager_input_properties_flat() {
    let bundle = build_bundle([fake_forager(
        "exec",
        "doc",
        json!({
            "cmd":  { "type": "string", "description": "shell command" },
            "cwd":  { "type": "string" },
        }),
    )]);

    let then_props = bundle
        .pointer("/definitions/ExperimentStepToml/allOf/0/then/properties")
        .and_then(|v| v.as_object())
        .expect("then.properties exists");
    assert!(then_props.contains_key("cmd"));
    assert!(then_props.contains_key("cwd"));
    assert!(then_props.contains_key("summary"));

    let required = bundle
        .pointer("/definitions/ExperimentStepToml/allOf/0/then/required")
        .and_then(|v| v.as_array())
        .expect("required forwarded");
    assert_eq!(required[0], "cmd");
}

#[test]
fn measurement_description_is_per_forager_doc() {
    let bundle = build_bundle([fake_forager(
        "cargo",
        "**time_ms** — wall-clock duration",
        json!({}),
    )]);

    let desc = bundle
        .pointer("/definitions/ExperimentStepToml/allOf/0/then/properties/summary/additionalProperties/allOf/1/properties/measurement/description")
        .and_then(|v| v.as_str())
        .expect("measurement description spliced");
    assert!(desc.contains("time_ms"));
}

#[test]
fn empty_foragers_does_not_inject_allof() {
    let bundle = build_bundle(std::iter::empty());
    assert!(
        bundle
            .pointer("/definitions/ExperimentStepToml/allOf")
            .is_none(),
        "empty foragers list should leave the step def untouched"
    );
}
