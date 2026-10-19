//! Every tool agents use, a file each, written by `npm run contracts:generate` from the
//! files here: a tool that needs features is in a build only while the MCP's Cargo feature of
//! each is on. A tool is a type implementing `toolbox::Tool`; `dispatchdev new tool <name>`
//! makes one that works as written, with its test. The connection, never the tool, says who
//! may use it.
#[cfg(feature = "driver_match")]
mod driver;
#[cfg(feature = "dvic")]
mod dvic_short_inspections;
mod get_profile;
mod support_ticket_diagnostics;
mod whoami;

use crate::toolbox::AnyTool;

/// Every tool, by its file's name.
pub const TOOLS: &[&dyn AnyTool] = &[
    #[cfg(feature = "driver_match")]
    &driver::Driver,
    #[cfg(feature = "dvic")]
    &dvic_short_inspections::DvicShortInspections,
    &get_profile::GetProfile,
    &support_ticket_diagnostics::SupportTicketDiagnostics,
    &whoami::Whoami,
];
