//! The operator slots: a feature's commands of the binary (`commands`), and the demo data it
//! fills a new DSP with (`demo`).
use crate::{Result, db::Store, foundation::config::Config};
use serde_json::Value;

/// Fills one DSP, given its timezone, with what a feature shows.
pub type Demo = fn(&Store, &str, &str) -> Result<()>;
/// A family of operator commands, run as `dispatch-backend <name> <arguments>` without the
/// platform lock, so each must be safe beside a running server.
pub struct Commands {
    /// What their names begin with. Every name so begun is theirs, and `run` answers one it
    /// does not know with its usage error.
    pub prefix: &'static str,
    /// Runs one, given every argument, its name first, and answers what it prints.
    pub run: fn(&Config, &[String]) -> Result<Value>,
}
