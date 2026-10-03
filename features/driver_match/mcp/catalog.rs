//! Driver Match's part of the agent catalog: what its codes are.
use dispatch_core::mcp::data::catalog::Term;

pub const TERMS: &[Term] = &[Term {
    term: "Driver Match code",
    meaning: "A six-character code for one person, the same across every source. Use it to name a driver exactly.",
    order: 10,
}];
