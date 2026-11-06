//! Slack-owned persistence in DSP databases and secrets, with workspace claims in the platform.
//! Only this crate writes SQL against its tables.
pub(crate) mod installation;
pub(crate) mod integrations;
pub(crate) mod logos;
pub(crate) mod members;
pub(crate) mod migrations;
pub(crate) mod workspaces;
