mod alias;
mod health;
mod setup;

pub use alias::{alias_cmd, load_aliases};
pub use health::health_cmd;
pub use setup::setup_cmd;
