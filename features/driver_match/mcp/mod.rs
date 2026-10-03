//! What agents can know of Driver Match: who its codes name.
mod catalog;

use crate::agents::Mcp;

pub const MCP: Mcp = Mcp {
    terms: catalog::TERMS,
    ..Mcp::NONE
};
