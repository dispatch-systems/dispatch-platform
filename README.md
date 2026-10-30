# Dispatch

Dispatch combines a Rust backend, React dashboard, and an email delivery Worker.
Run commands from the repository root.

| Directory                   | Owns                                                                                 |
| --------------------------- | ------------------------------------------------------------------------------------ |
| `core/`                     | The platform, in parts; names no collector or feature                                |
| `collectors/`               | One directory per outside site: `paycom/`, `cortex/`                                 |
| `features/`                 | One directory per feature: backend, API, frontend, migrations and tests              |
| `mcp/`                      | The MCP, whole: how outside agents connect, the tools they use and the Agents page   |
| `app/`                      | The assembly: the `dispatch-backend` binary, the frontend entry, whole-product tests |
| `services/cloudflare-mail/` | Email Worker and its generated environment types                                     |
| `tooling/`                  | Build, CI, dev, scaffolding, testing, agent eval, asset, security and mail helpers   |
| `ops/`                      | Host manager, launchers, systemd units and host scripts                              |

Dependencies point one way: `app → mcp → features → collectors → core`. Core, each collector,
each feature, the MCP and the app is its own crate, and each feature and collector declares what it
contributes in its manifest (`feature.rs`, `collector.rs`). Only `app/` lists them: its list of
features, and their place in its Cargo manifest, are written from the folders in `features/`, so
adding a feature is adding its folder, and its `place` in its manifest sets where it is listed.
Rust owns punch interpretation and meal assessment; the dashboard formats typed assessment
results. Each owner's frontend fills the shell's slots from `frontend/feature.ts`, which loads
its `index.ts` lazily, and the platform owner's pages from `frontend/platform-slots.ts`; only
`app/frontend/features.ts` lists them. Styles live with their owner;
`core/shell/frontend/styles.css` sets the global import order. API types and tooling never
import frontend code.

## Development

`./setup` makes a clone ready on Linux or macOS: the Node `.node-version` pins, the Rust
`rust-toolchain.toml` names, the packages, the first build, and the private agent docs, linked
where Claude and Codex read them. It needs git, Python 3 and a C compiler, and runs again at any
time. `dispatchdev` is `tooling/cli/dispatchdev`, built on its first run.

```sh
git clone https://github.com/dispatch-systems/dispatch-platform.git
cd dispatch-platform && ./setup
npm run dev            # the dashboard on demo data, at the link it prints
git switch -c <name>   # then change it, and commit
dispatchdev check      # the rule checks, the linter and clippy, side by side
gh pr create           # then, once its checks pass: gh pr merge --squash
```

The PR's checks run every suite on GitHub, and it's squash-merged once they pass; Dev installs
each merge. Its title and body become the squash commit, and the release notes take
its Summary. While `npm run dev` runs, `dispatchdev shot --page <page>` saves a PNG of a page
and `dispatchdev api GET /api/session` makes a signed-in call; a backend change needs
`npm run dev` started again.

Every owner keeps its tests in `tests/<kind>/`. `dispatchdev test` runs the ones a change
touches, `--owner <owner>` one owner's of every kind; `npm test` discovers the TypeScript tests,
and the Python tests run with `python3 -m unittest discover -s tooling/tests -p '*_test.py'` and
`-s ops/tests`. `dispatchdev new feature <name>` and `dispatchdev new collector <site>` start a
feature or a collector that already builds and passes its checks, and `dispatchdev new tool
<name>` a tool agents can use, in `mcp/tools/`. `dispatchdev list` and `dispatchdev explain
<owner>` say what the product has and what an owner is made of.

`dispatchdev generate` lists the features in `app/`, then writes each owner's API types from
Rust into its `api/generated/`, which the owner's `api/` narrows and checks, and the snapshots
the app's tests hold. Normal Rust tests verify them without rewriting
files. Generated types, schema snapshots and approved artwork remain with their owners.

Installed hosts run their installed copies of `ops/launchers/` and the built artifact, never
repository paths. Building the backend and checking a branch before its PR run through
`dispatchdev` (`tooling/cli/`), and artifact verification, releases, fresh setup and the
updaters through the Rust host manager (`ops/host-manager/`); what both use is in
`tooling/shared/`. See [tooling/ci/README.md](tooling/ci/README.md) for the pipeline.

## License

Copyright 2026 Dillon Lillehaug. Dispatch is source-available under the Functional Source
License, version 1.1, with Apache 2.0 as its future license ([LICENSE](LICENSE)). Anyone may use,
change and share it, including running it for their own business, but not offer it or anything
built from it to others as a competing commercial product or service, hosted or not. Two years
after each version is released, that version is also available under the Apache License 2.0. The
dashboard's account menu links each running build to its source.
