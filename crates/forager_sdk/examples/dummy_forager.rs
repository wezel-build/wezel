use forager_sdk::{Forager, forager_main};

struct DummyForager;

impl Forager for DummyForager {
    const NAME: &'static str = "dummy";

    const DESCRIPTION: &'static str = "An example forager";

    const OUTCOMES_DOC: &'static str = "";

    type Inputs = ();

    fn run(_: Self::Inputs) -> anyhow::Result<Vec<wezel_types::ForagerPluginOutput>> {
        Ok(vec![])
    }
}

forager_main!(DummyForager);
