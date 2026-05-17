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
fn bundle_has_one_step_property_per_forager() {
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

    let step_props = bundle
        .pointer("/properties/step/properties")
        .and_then(|v| v.as_object())
        .expect("step.properties exists");
    assert!(step_props.contains_key("exec"));
    assert!(step_props.contains_key("cargo"));

    // Unknown tools should be flagged in the editor.
    assert_eq!(
        bundle.pointer("/properties/step/additionalProperties"),
        Some(&serde_json::Value::Bool(false)),
    );

    // Each tool entry points at a per-tool definition.
    let exec_ref = bundle
        .pointer("/properties/step/properties/exec/additionalProperties/$ref")
        .and_then(|v| v.as_str())
        .expect("exec $ref present");
    assert_eq!(exec_ref, "#/definitions/Step_exec");
    assert!(bundle.pointer("/definitions/Step_exec").is_some());
    assert!(bundle.pointer("/definitions/Step_cargo").is_some());
}

#[test]
fn step_def_combines_common_fields_with_forager_inputs() {
    let bundle = build_bundle([fake_forager(
        "exec",
        "doc",
        json!({
            "cmd":  { "type": "string", "description": "shell command" },
            "cwd":  { "type": "string" },
        }),
    )]);

    let props = bundle
        .pointer("/definitions/Step_exec/properties")
        .and_then(|v| v.as_object())
        .expect("Step_exec.properties exists");
    // Common StepBody fields.
    assert!(props.contains_key("description"));
    assert!(props.contains_key("apply-diff"));
    assert!(props.contains_key("summary"));
    // Forager inputs.
    assert!(props.contains_key("cmd"));
    assert!(props.contains_key("cwd"));

    let required = bundle
        .pointer("/definitions/Step_exec/required")
        .and_then(|v| v.as_array())
        .expect("required forwarded");
    assert!(required.iter().any(|v| v == "cmd"));

    // Closed door for typos.
    assert_eq!(
        bundle.pointer("/definitions/Step_exec/additionalProperties"),
        Some(&serde_json::Value::Bool(false)),
    );
}

#[test]
fn measurement_description_is_per_forager_doc() {
    let bundle = build_bundle([fake_forager(
        "cargo",
        "**time_ms** — wall-clock duration",
        json!({}),
    )]);

    let desc = bundle
        .pointer("/definitions/Step_cargo/properties/summary/additionalProperties/allOf/1/properties/measurement/description")
        .and_then(|v| v.as_str())
        .expect("measurement description spliced");
    assert!(desc.contains("time_ms"));
}

#[test]
fn empty_foragers_returns_base_schema_unchanged() {
    let bundle = build_bundle(std::iter::empty());
    // No per-tool defs and no narrowed `step.properties` when there are no
    // foragers — the schemars-derived base passes through.
    assert!(bundle.pointer("/properties/step/properties").is_none());
    assert!(bundle.pointer("/definitions/Step_exec").is_none());
}
