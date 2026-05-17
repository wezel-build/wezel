use forager_sdk::Forager;
use schemars::JsonSchema;
use serde::Deserialize;
use wezel_types::ForagerPluginOutput;

#[derive(Deserialize, JsonSchema)]
struct DummyInputs {
    _cmd: String,
}

struct Dummy;

impl Forager for Dummy {
    const NAME: &'static str = "dummy";
    const DESCRIPTION: &'static str = "test forager";
    const OUTCOMES_DOC: &'static str = "Emits `wall_time` and `rss_peak`.";
    type Inputs = DummyInputs;

    fn run(_inputs: Self::Inputs) -> anyhow::Result<Vec<ForagerPluginOutput>> {
        Ok(vec![])
    }
}

#[test]
fn inputs_schema_is_well_formed() {
    let schema = Dummy::inputs_schema();
    let obj = schema.as_object().expect("schema is an object");
    assert!(obj.contains_key("properties") || obj.contains_key("$ref"));
}

#[test]
fn outcomes_doc_is_exposed() {
    assert!(Dummy::OUTCOMES_DOC.contains("wall_time"));
}
