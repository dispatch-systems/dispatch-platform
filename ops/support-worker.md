# AI Support across Dev and Production

AI Support routes, tickets and owner decisions run in each platform's own database. A single
worker on `dispatch-dev` consumes both queues. The checked artifact contains one common
`services/support-worker/hermes` bundle; every origin uses its identical analyst, builder
and reviewer profile templates, skills, custom tools and model settings. Ticket-specific
profile instances isolate sessions; an environment changes context and authority only.

The application never installs Hermes on Production. Production investigation is read-only,
and collection verification uses the configured Dev verification account in a separate
private DSP. Merged changes reach Dev through the existing checked-artifact updater. Production changes arrive through
the separately published release, followed by read-only deployment verification.

The Dev artifact includes:

- `services/rust/dispatch-support-worker` and the common Hermes resources;
- `services/support-worker/support-collection.py`, a trusted credential-reference wrapper;
- `services/support-worker/dispatch-support-worker.service`, the Dev-only user service.

The first supporting artifact needs a compatible host manager: older managers reject its
new resource paths before they can adopt the manager embedded in that artifact. From the
trusted checkout of the merged, checked supporting commit, build and run its standalone
manager once. It uses the ordinary update lock, queue validation, artifact hashing, service
health check and rollback path; successful installation then refreshes the installed manager.
It does not change the live checkout before those checks pass.

```sh
cargo build --locked --release -p dispatch-host
target/release/dispatch-host host dev --root "$HOME/dispatch-platform/dev"
target/release/dispatch-host host dev --root "$HOME/dispatch-platform/dev" --verify-management
```

If checks are still pending, run the ordinary update command again after they pass. Running
`--install-management` before this first update would copy the old active manager. Keep the
previous management copy for rollback; do not bypass the artifact allowlist or install an
unverified candidate executable.

Production has the same first-artifact compatibility transition. During a separately
authorized release rollout, build the standalone manager from that trusted source commit,
copy it over authenticated SSH into a private staging directory on Production, and run it
once in ordinary Production mode:

```sh
chmod 700 "$HOME/.local/share/dispatch-rollout/dispatch-host"
"$HOME/.local/share/dispatch-rollout/dispatch-host" host production --root "$HOME/dispatch-platform/public"
"$HOME/.local/share/dispatch-rollout/dispatch-host" host production --root "$HOME/dispatch-platform/public" --verify-management
```

Create the staging directory with mode `0700`, compare its transferred SHA-256 with the
trusted locally built binary, and remove it after successful adoption. The new manager
still requires the normal immutable published release, provenance, attestation, archive
digest, source binding and health checks. This step does not publish a release and must not
run until the Production rollout itself is authorized. Production never enables a worker.

Run the installed host manager's `host support --root $HOME/dispatch-platform/dev
--install-unit` to install the unit from a verified active artifact. The command installs
only on Dev and does not enable a worker without its private configuration. The Dev updater
uses `try-restart` after successful activation, so an existing worker adopts the checked
resource bundle and interrupted work resumes through durable leases. An inactive worker
stays inactive. Production updates never start or restart the worker.

The private configuration at `$HOME/dispatch-platform/.runtime/support-worker/config.json`
uses separate dedicated bearer files for the Dev and Production origins. Both origins are
explicitly configured with `environment` equal to `dev` or `production`. Never put bearer
values in the repository, command arguments, profiles or Hermes tool environments.

The private `collection-reference.json` contains only the current Dev state root and the
verification DSP ID, as `sourceRoot` and `sourceDsp`. It references the existing vault. No persistent
copy of a verification password or OAuth refresh token is made. `support-collection.py`
checks the assigned worktree and build commit, seeds fresh private state, and invokes that branch's
normal collector/job/keeper path. The trusted wrapper projects only the selected account
and provider metadata plus temporary encrypted vault material into a private broker root.
A network-isolated process sandbox mounts that projection, the branch artifact and isolated
state; it cannot read host home directories, live DSP roots or worker authentication. Vault
material resides on RAM-backed broker storage and an isolated tmpfs target vault. The
installed collector registry brokers only that provider's permitted public HTTPS hosts.
Only the isolated collector decrypts credentials. A shared
OS account lease excludes normal verification-account provider browsers while verification
runs; source credential rotation or disablement cancels the run. Temporary encrypted credentials are
deleted afterward. Changed collector output remains in private isolated state so the
reviewer can exercise the actual collected rows in a real browser. Logs and raw provider
records never become agent-tool or GitHub responses.

The bot's optional `support_prs` feature reads worker-written private registrations with
the approved ticket version, source origin, PR, branch and tested head. It delivers short,
durable GitHub outcomes to `/api/v1/support/worker/github/events` using separate
`SUPPORT_GITHUB_TOKEN` bearer files. The platform rechecks its stored approval and PR binding.
The bot runs no Hermes jobs on its serial event bus and grants no approval or merge authority.

Enable live account verification only after Dev runs the account-lease-aware collector version,
so scheduled collections and verification share the same exclusion mechanism. If a provider
requires MFA or becomes unavailable, the ticket waits with its evidence rather than passing.
The existing provider account protections and physical scheduling capacity remain in force.
There are no application investigation, token, spending or attempt quotas.
