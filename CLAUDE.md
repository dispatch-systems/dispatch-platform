# CLAUDE.md

**These instructions are ONLY for Claude Duet** (Claude working here through the Claude Duet app).
Every other agent or session ignores this file.

`.claude/hooks/session-start.sh` installs the crates and npm packages when a cloud session starts.

- Rules, types, format and source lints: `npm run check:rules`
- Types: `npm run check`; lint: `npm run lint`
- Rust: `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`,
  `cargo test --locked -p <crate>`
- One owner's tests: `npm run test:feature -- <name>` (also `test:collector`, `test:core`,
  `test:mcp`)

The browser suites need a browser, and the sandbox tests in `ops/tests` need bubblewrap, which a
cloud session doesn't have.
