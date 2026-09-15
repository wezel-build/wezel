use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

/// Fields valid in `.wezel/config.toml` (project scope).
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Stable project identity (generated once by `wezel project init`).
    pub project_id: uuid::Uuid,
    /// Human-readable project name.
    pub name: String,
    /// List of registry URIs for experiment adapters.
    /// Each entry can be any valid URI (https://, file://, etc.).
    pub registries: Option<Vec<String>>,
    /// `[tools]` umbrella — only the bits init/sync need from this side. The
    /// canonical schema lives in `wezel_bench::ToolsSection`; foragers are
    /// read through that.
    #[serde(default, skip_serializing_if = "ToolsConfig::is_empty")]
    pub tools: ToolsConfig,
}

/// Minimal `[tools]` view for the init-side config writer. Round-trips the
/// `targets` list; existing `[tools.foragers.*]` sections deserialize fine
/// because unknown fields are ignored.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default, skip_serializing_if = "IndexSet::is_empty")]
    pub targets: IndexSet<String>,
}

impl ToolsConfig {
    fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
}
