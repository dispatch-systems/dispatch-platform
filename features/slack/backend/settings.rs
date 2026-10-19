//! Slack's section of Settings, on its own page: its workspaces' settings.
use dispatch_core::manifest::{Section, section};

/// Each of the DSP's workspaces, chosen one at a time: the channels Dispatch is in, and the
/// integrations switched on there. Slack's own component draws it, as none of it is a value
/// Settings keeps.
pub const WORKSPACES: Section = section("slack.workspaces", "Workspaces", "slack.manage", &[])
    .custom()
    .keywords(&["channels", "integrations", "workspace"]);
