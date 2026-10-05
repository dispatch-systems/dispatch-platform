# Dispatch

Dispatch combines a Rust backend, React dashboard, and an email delivery Worker.
Run commands from the repository root.

| Directory                   | Owns                                                                                                      |
| --------------------------- | --------------------------------------------------------------------------------------------------------- |
| `core/`                     | The platform, in parts; names no collector or feature                                                     |
| `collectors/`               | One directory per outside site: `paycom/`, `cortex/`                                                      |
| `features/`                 | One directory per feature: backend, API, MCP, frontend, migrations and tests                              |
| `app/`                      | The assembly: the `dispatch-backend` binary, the frontend entry, whole-product tests                      |
| `services/cloudflare-mail/` | Email Worker and its generated environment types                                                          |
| `tooling/`                  | Build, CI, dev, scaffolding, testing, agent eval, benchmark, asset, security, mail and screenshot helpers |
| `ops/`                      | Host manager, launchers, systemd units and host scripts                                                   |

Dependencies point one way: `app → features → collectors → core`. Core, each collector, each
feature and the app is its own crate, and each feature and collector declares what it
contributes in its manifest (`feature.rs`, `collector.rs`). Only `app/` lists them. Rust owns
punch interpretation and meal assessment; the dashboard formats typed assessment results. Each
owner's frontend fills the shell's slots from `frontend/feature.ts`, which loads its `index.ts`
lazily, and the platform owner's pages from `frontend/platform-slots.ts`; only
`app/frontend/features.ts` lists them. Styles live with their owner;
`core/shell/frontend/styles.css` sets the global import order. API types and tooling never
import frontend code.

Use an isolated worktree branched from `origin/main`, then `npm ci` and `npm run dev`.
The preview prints a private fixture URL. `npm run check:rules` runs the structure rules and the
other source checks before a push. `npm run check:ci` runs full validation;
`npm run check:ci -- checks` runs the dashboard checks against a build. Every owner keeps its
tests in `tests/<kind>/`: `npm test` discovers the TypeScript tests, and
`npm run test:feature -- <name>`, `test:collector -- <site>` and `test:core -- <part>` run one
owner's tests of every kind. Python tests use
`python3 -m unittest discover -s tooling/tests -p '*_test.py'` and `-s ops/tests`.
`npm run new:feature -- <name>` and `npm run new:collector -- <site>` start a feature or a
collector that already builds and passes its checks.

`npm run contracts:generate` writes each owner's API types from Rust into its `api/generated/`,
which the owner's `api/` narrows and checks. Normal Rust tests verify them without rewriting
files. Generated types, schema snapshots and approved artwork remain with their owners.

Installed hosts run their installed copies of `ops/launchers/` and the built artifact, never
repository paths. Building the backend, checking a branch before a push and shipping a PR run
through `dispatchdev` (`tooling/cli/`), and artifact verification, releases, fresh setup and
the updaters through the Rust host manager (`ops/host-manager/`); what both use is in
`tooling/shared/`. See [tooling/ci/README.md](tooling/ci/README.md) for the pipeline. The
development workflow lives outside Git, in the workspace’s `dispatch-development` skill.

## License

Copyright 2026 Dillon Lillehaug. Dispatch is source-available under the Functional Source
License, version 1.1, with Apache 2.0 as its future license ([LICENSE](LICENSE)). Anyone may use,
change and share it, including running it for their own business, but not offer it or anything
built from it to others as a competing commercial product or service, hosted or not. Two years
after each version is released, that version is also available under the Apache License 2.0. The
dashboard's account menu links each running build to its source.
