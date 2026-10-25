# CLAUDE.md

**These instructions are ONLY for Claude Duet**: Claude working in this repository through the
Claude Duet app, in a Claude Code cloud session. Any other agent or Claude session, including
Claude Code on the owner's own machines and anything working through the `dispatch-development`
skill, ignores this file and follows its own workflow.

[README.md](README.md) says how the repository is laid out and
[tooling/ci/README.md](tooling/ci/README.md) what CI runs. Run every command from the repository
root.

## Setup

`.claude/hooks/session-start.sh` runs when a cloud session starts: it installs the Rust toolchain
`rust-toolchain.toml` pins, fetches the crates in `Cargo.lock` and runs `npm ci` when
`package-lock.json` changed. If it reports a failure, run it again with
`CLAUDE_CODE_REMOTE=true .claude/hooks/session-start.sh`. Python tooling uses the standard
library only. npm may warn `EBADENGINE` when the session's Node is older than `engines.node`; the
install still works.

## Working on a change

Work on the branch the session names, commit there and open the PR through GitHub. Leave
`dispatchdev start`, `preview`, `ship` and `finish` alone: they manage worktrees, previews and
the Dev install on the owner's machine. CI runs in the merge queue (`platform` is the gate), so a
PR is checked once it is queued.

## Checks

| What                                    | Command                                                                     |
| --------------------------------------- | --------------------------------------------------------------------------- |
| Everything a push can fail on, no build | `npm run check:rules` (about a minute)                                      |
| TypeScript types                        | `npm run check`                                                             |
| Lint                                    | `npm run lint`, or `npx eslint <path>` for one place                        |
| Format                                  | `npm run format:check`; `npm run format` rewrites                           |
| Rust                                    | `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`   |
| Rust tests, one crate                   | `cargo test --locked -p <crate>`, e.g. `dispatch-timecard`, `dispatch-core` |
| One owner's tests of every kind         | `npm run test:feature -- <name>`, `test:collector -- <site>`,               |
|                                         | `test:core -- <part>`, `test:mcp`; add `--only rust\|api\|frontend`         |
| One TypeScript test file                | `npx tsx --test <file>`                                                     |
| Python tooling tests                    | `python3 -m unittest discover -s tooling/tests -p '*_test.py'`              |
|                                         | and `-s ops/tests`                                                          |
| After changing Rust API types           | `npm run generate`, then commit what it writes                              |

`npm test`, `npm run test:ui` and `npm run check:ci` build the whole backend and run the full
suites; they take a long time here, so run them only when a change needs them. The browser and
native collector suites need a browser this session may not have, and the support sandbox tests
in `ops/tests` need bubblewrap (`/usr/bin/bwrap`), which a cloud session lacks unless installed.
