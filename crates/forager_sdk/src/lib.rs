//! SDK for writing forager plugins.
//!
//! A forager is a binary that the wezel runner invokes once per step. The
//! binary takes its inputs as JSON via the `FORAGER_INPUTS` env var, runs
//! whatever procedure it implements, and writes a [`ForagerPluginEnvelope`]
//! of outcomes to the path in `FORAGER_OUT`. It must also respond to a
//! `--schema` flag with its self-description so the wezel CLI can compose
//! editor-facing JSON Schemas for `experiment.toml`.
//!
//! Implementors define a unit type and `impl Forager for ...`, then invoke
//! [`forager_main!`] to generate the binary entry point.

use std::path::PathBuf;

use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use wezel_types::{ForagerPluginEnvelope, ForagerPluginOutput, ForagerSchema};

/// Contract implemented by every forager binary.
pub trait Forager {
    /// Forager identifier as it appears in the `[step.<tool>.<name>]` header
    /// of an experiment TOML file.
    const NAME: &'static str;
    /// One-line description shown in `wezel experiment new` and tool listings.
    const DESCRIPTION: &'static str;
    /// Markdown documenting the outcomes this forager emits (names, value
    /// units, available filter tags). Spliced into the `description` of the
    /// `outcome` field in the bundled `.wezel/schema.json`, so editors
    /// surface it on hover once a step's tool is known.
    const OUTCOMES_DOC: &'static str;

    /// Inputs deserialised from `FORAGER_INPUTS`.
    type Inputs: DeserializeOwned + JsonSchema;

    fn run(inputs: Self::Inputs) -> Result<Vec<ForagerPluginOutput>>;

    /// JSON Schema for [`Self::Inputs`]. Default impl derives it via schemars;
    /// override only to post-process the generated schema.
    fn inputs_schema() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(Self::Inputs))
            .expect("schema serialization is infallible")
    }
}

/// Entry point used by [`forager_main!`]. Handles `--schema`, reads
/// `FORAGER_INPUTS`, invokes [`Forager::run`], and writes the envelope to
/// `FORAGER_OUT`.
pub fn run<F: Forager>() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--schema") {
        let schema = ForagerSchema {
            name: F::NAME.into(),
            description: F::DESCRIPTION.into(),
            inputs: F::inputs_schema(),
            outcomes_doc: F::OUTCOMES_DOC.into(),
        };
        println!("{}", serde_json::to_string_pretty(&schema)?);
        return Ok(());
    }

    let inputs_path: PathBuf = std::env::var_os("FORAGER_INPUTS")
        .context("FORAGER_INPUTS not set")?
        .into();
    let out_path: PathBuf = std::env::var_os("FORAGER_OUT")
        .context("FORAGER_OUT not set")?
        .into();

    let inputs_raw = std::fs::read_to_string(&inputs_path)
        .with_context(|| format!("reading {}", inputs_path.display()))?;
    let inputs: F::Inputs = serde_json::from_str(&inputs_raw).context("parsing FORAGER_INPUTS")?;

    let outcomes = F::run(inputs)?;

    let envelope = ForagerPluginEnvelope { outcomes };
    let body = serde_json::to_string(&envelope).context("serialising envelope")?;
    std::fs::write(&out_path, body).with_context(|| format!("writing {}", out_path.display()))?;
    Ok(())
}

#[doc(hidden)]
pub fn __main<F: Forager>() {
    if let Err(e) = run::<F>() {
        eprintln!("forager-{}: {e:#}", F::NAME);
        std::process::exit(1);
    }
}

/// Generate the `fn main` for a forager binary.
///
/// ```ignore
/// struct Exec;
/// impl forager_sdk::Forager for Exec { /* ... */ }
/// forager_sdk::forager_main!(Exec);
/// ```
#[macro_export]
macro_rules! forager_main {
    ($ty:ty) => {
        fn main() {
            $crate::__main::<$ty>()
        }
    };
}
