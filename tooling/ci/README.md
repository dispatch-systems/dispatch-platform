# CI tooling

`.github/workflows/checks.yml` is the whole pipeline. It runs every suite a PR's change can
reach on the PR, and every suite again on each push to `main`, on the squash commit the PR
became. `platform` is the one required check: a PR can merge once it passes. `tooling/ci/checks.ts` runs one of its jobs by name, or
locally the whole suite in sequence:

| Job              | `npm run check:ci -- …`                    | What it proves                                                                            |
| ---------------- | ------------------------------------------ | ----------------------------------------------------------------------------------------- |
| build            | `build`                                    | The runtime packages: the release backend and worker, and the dashboard.                  |
| checks           | `checks`                                   | Source privacy/secrets, types, formatting, the linter and dashboard logic.                |
| browser ×6       | `browser <n>/6 [spec]`                     | The browser suite against the packaged runtime.                                           |
| smoke            | `smoke`                                    | The package's manifest, build paths and budgets, and that it starts, signs in and serves. |
| core-lint        | `core lint`                                | Rust formatting, lints and compile-only operator probes.                                  |
| core             | `core tests`                               | The Rust tests, through nextest (`nextest.py`), and the doctests.                         |
| debug            |                                            | The debug backend and worker, built once for api and the collectors.                      |
| api              | `api`                                      | The API tests, the Python tooling tests, the npm audit.                                   |
| scaffold-lint    | `scaffold lint`                            | What every generator writes builds, lints and keeps the rules.                            |
| scaffold         | `scaffold tests`                           | What every generator writes passes its Rust tests and its agent tools' tests.             |
| collectors ×6    | `npm run test:browseros -- --shard <name>` | The native collectors with a real browser.                                                |
| rust-advisories  |                                            | `cargo audit`, denying yanked crates but those `cargo-audit.py` allows.                   |
| relay-dev        |                                            | Outside a PR, Dev's relay app, signed, for the build to pack (`relay.py`).                |
| relay-production |                                            | In a release's run that names its app version, Production's relay app, signed.            |
| changes          |                                            | Which suites the change can reach (`changes.py`).                                         |
| platform         |                                            | The gate: the one required check.                                                         |

On a PR, `changes.py` skips a suite when every file the change touches is of a kind it never
reads: the Android apps, Markdown outside `services/`, or, for core and the collectors, a
`frontend/` folder's code but the owners' manifests, and for core alone, any other TypeScript or
CSS but the generated API bindings and CI's own scripts; the Android apps' job runs only for them and
for CI's own files, on a push to `main` too. Anything it can't read runs every suite. core,
scaffold and their -lint jobs, the longest, decide for themselves instead of waiting for it, and
pass without running when not needed; the gate accepts any other suite skipped only where
`changes` said so, and its summary names each job's result and what the change touched.

The debug backend and worker are built once a run: by the `debug` job, when `changes` finds the
change reaches what they compile from, and saved for api and the collectors to restore through
`dispatchdev build`'s exact-input cache. Otherwise `debug` skips, and they restore the build
`main`'s own run saved, without waiting for one. The capacity shard, which also compiles the
host's tests, restores Cargo's dependency cache too.

The gate passes only when every job passed, the relay apps' builds wherever they ran, and the
package carries the run's own commit and a complete inventory (`ci-verify.py`, which runs
`dispatch-host ci verify`). Outside a PR it then attests the package and publishes it as `dispatch-main-<sha>` for 90 days: the Dev updater
installs `main`'s, which carries Dev's app. A release dispatches its own run on a temporary branch
at its commit, which builds or carries Production's app beside Dev's, and stamps its version into
that run's package ([Android apps](../../android/README.md#releases)). Once `main`'s push run of
the commit passed, the release names it (`tested-run`), and its run skips checks, core-lint,
core, debug, api, collectors, scaffold-lint, scaffold and browser, the suites that tested the source there, and keeps build and
smoke, which test its own package; the gate accepts them skipped only once GitHub answers that the
named run is `main`'s passed push run of the same commit.

A PR's newer push cancels its older run; each commit of `main` keeps its own. A failure that
isn't the change's own gets its failed jobs run again: `gh run rerun <id> --failed`.
`caches.yml` refills `main`'s caches on every push to it and each Monday, and `security.yml`
audits the dependencies nightly and runs CodeQL on each push to `main`; neither gates a merge.

`dispatchdev` (`tooling/cli/`) holds what runs on a developer's machine, and `dispatchdev help`
lists it. `start` makes a change's worktree beside the clone, `pr` checks its PR's title and
body before opening it, and `finish` removes the change once it merged. `check`, `test` and
`verify` look at a branch before its PR; `api` (signed-in calls)
and `shot` (a PNG of a page, signed in once for each account) reach the dev server `npm run dev`
runs in the checkout, which records itself in `.runtime/dev-server.json` while it runs. A Mac
runs none of the local checks and tests (`tooling/cli/src/machine.rs`
says why): there `check`, `test` and `verify` refuse, and the checks on the PR decide alone.
`dispatchdev build` builds the package's two executables, the backend and the support worker,
in one Cargo pass, through a cache keyed by their exact inputs and the compiler's fingerprint,
and packs the relay app's APKs CI signed, from the folder `DISPATCH_RELAY_APPS` names, in
`dashboard/relay/` ([Releases](../../android/README.md#releases)), which `npm run build` copies into
the package.
`dispatchdev test` runs the tests a change touches, committed or not, or `--all` of the Rust and
API tests, one at a time, a line each. Each run builds in a folder of its own under
`.test-build/`, without incremental data, and deletes it when it ends, with the release build
and package it made for the browser tests (`--keep` keeps them), so worktrees hold no test
builds and several can test at once. Its commands, like `check`'s and `verify`'s, run in
systemd scopes of one slice, `app-dispatchdev.slice`, which shares a low CPU weight and a
memory ceiling between them, so Dev keeps priority. They are the first the out-of-memory killer
ends, before the agents' sessions, and stop when the session that started them ends.
`dispatchdev test --owner <owner>` runs one owner's tests of every kind the same way, as
`tooling/testing/owner-tests.ts` lists them, with the debug package for its browser tests;
`--only` keeps one kind and `--file` one file. `dispatchdev verify <feature>` looks at a feature
in progress, committed or not: `generate --check`, what a generator left to write, the rules,
TypeScript, the changed files' format and lint, then its own tests as `test --owner` runs them.
`dispatchdev check` stops a branch that isn't ready to push, on `main` or with changes not yet
committed, then runs the rule checks (`npm run check:rules`, everything a push can fail on
without a build), the TypeScript linter and clippy side by side; clippy builds in the
checkout's own `target`, so a run after a small change takes seconds. A clippy pass is kept, in the repository's shared Git
folder, for the Rust inputs it ran on, so clippy passes again without running until one of them
changes, and a whole pass is kept for the committed tree it ran on, so the same tree passes
again at once; `--plan` only names them.

Caches: only `main`'s reach every branch, since a PR's run saves caches only that PR reads.
`caches.yml` refreshes them on every push to `main`: the release executables keyed by their
Rust inputs, which the build job restores instead of compiling when a branch's Rust is `main`'s
(`main`'s run of the `debug` job saves the debug ones), the CI and host tools, the assessment fixture, the Playwright browser, the
BrowserOS package and the installed packages, which every job restores through
`.github/actions/setup-packages` instead of running `npm ci`. A run's own `tools` job builds
what its inputs lack for the jobs that start later in that run. GitHub keeps 10 GB of caches
and evicts the least recently used past that, so once `main`'s are saved, `caches.py prune`
deletes those newer ones of the same kind replaced and nothing read for a day, CodeQL's
whatever their last read, since only `main`'s analyses write them and nothing reads them, and
closed PRs' caches; `pr-caches.yml` deletes a PR's own as soon as it closes. A build job that compiled keeps
Cargo's `--timings` report of where the compile spent its time as an artifact.

`tooling/cli/dispatchdev` runs a restored copy only on CI and only from the
workspace's own `.ci-tools` directory. Anywhere else it builds the binary once for each
version of its source, beside the repository's shared Git data, for every worktree to reuse.

The repository Cargo config runs `tooling/build/rustc-remap.py` for dependencies and workspace
crates, giving compiler paths neutral `/dispatch-build/...` prefixes. The wrapper's SHA-256
in `build.rustflags` invalidates Cargo's dependency objects when the policy changes; update
the hash after editing the wrapper. `check:rules` verifies it. Binary-cache schema 4 and the
tool/fixture keys fingerprint the wrapper and config. Only this exact config allows binary
reuse; custom configs, wrappers or overriding Rust flags (including empty and target-specific
environment flags) disable it. Packaging and the
artifact test run `tooling/security/check-build-paths.py` even for a reused executable, and
reject remaining local build paths without printing their contents. The first build after
adopting this policy recompiles dependencies. Compiler diagnostics use the neutral paths too.

Run the tests with:

```sh
cargo test --locked -p dispatchdev-cli -p dispatch-shared -p dispatch-host
python3 -m unittest discover -s tooling/tests -p '*_test.py'
python3 -m unittest discover -s ops/tests -p '*_test.py'
```

`check:rules` runs the source-only Python partition and every dashboard helper test without
compiling Rust. The dashboard tests are every owner's `tests/frontend/`, found where they
live, so a feature's own run without a list naming them. Each rule test, and each hook, has
60 s, and its file ends once its tests have, whatever it still holds open. A file with a test
that timeout cancelled, as a machine busy with other builds can, runs once more alone after the
other checks end, and only a failure, or a second timeout, fails it. The API job runs those
Python modules and the separate real compiler and
installed-manager modules once each. Native collector files run only in their collector
shards, so the API job does not launch them merely to skip their browser cases.
`tooling/tests/test-plan.test.ts` checks complete, disjoint file coverage against the commands
the runners actually use. Real compiler/manager modules are listed under `pythonIntegration`
in `test-plan.json`; the default unittest discovery command above still runs every module.
Live Rust operator probes require the explicit `operator-probes` feature; core CI typechecks
them without executing them.

`npm run check:privacy` scans publishable working files, also through `check:rules` before
pushes and the required `checks` job. It downloads the checksum-pinned Gitleaks release in
`tooling/security/gitleaks.json`, checks for private paths, emails and known identifiers,
and requires exact hashes for reviewed binary assets. Findings show locations and rules,
never matched values. The scan excludes private ignored state and does not cover Git history
or external exports; review prose and images manually. Do not broaden policy exceptions to
silence a finding.
