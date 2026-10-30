# AI Support across Dev and Production

AI Support routes, tickets and owner decisions run in each platform's own database. A single
worker on `dispatch-dev` consumes both queues. The checked artifact contains one common
`services/support-worker/hermes` bundle; every origin uses its identical analyst, builder
and reviewer profile templates, skills, custom tools and model settings. Ticket-specific
profile instances isolate sessions; an environment changes context and authority only.

The application never installs Hermes on Production. Production investigation is read-only,
including its reads of Production's server (below), and collection verification uses the configured Dev verification account in a separate
private DSP. Merged changes reach Dev through the existing checked-artifact updater. Production changes arrive through
the separately published release, followed by read-only deployment verification.

The Dev artifact includes:

- `services/rust/dispatch-support-worker` and the common Hermes resources;
- `services/support-worker/support-collection.py`, a trusted credential-reference wrapper;
- `services/support-worker/dispatch-support-worker.service`, the Dev-only user service.

Each new report starts with a tool-free intake stage from the common analyst profile. It
receives ticket text, conversation and reporter/DSP context before the worker assigns source,
issues a browser session or builds a preview. Explicit tests and acknowledgments receive a
reply; simple questions use existing context or read-only source when needed. A message that
describes an actual failure still gets investigated even when it also says "test".

Investigations select source inspection, a read-only browser or an isolated reproduction
preview according to the evidence needed. A preview is built only when reproduction needs
to change data. Source-only stages cannot invoke the browser tool. Skipped browser work is
recorded as skipped, never as successful verification. Approved implementations retain their
independent browser verification, and merge only once their PR's required checks pass.

Vague bug reports and feature requests receive a best-effort investigation. If the intended
issue or feature remains unclear, the ticket becomes **Unclear Bug Report** or **Unclear
Feature**, with findings and focused questions. An identified bug that cannot be reproduced
keeps its explicit uncertainty; access or provider failures remain waiting conditions.
Reporter clarifications resume intake, and ordinary acknowledgments do not repeat the
investigation. Replies and notifications remain in the platform. Intake outcomes require no
implementation proposal and grant no approval or publication authority.

The first supporting artifact needs a compatible host manager: older managers and their
Python startup guards reject its new resource paths. From the trusted checkout of the merged,
checked supporting commit, build a standalone manager. A temporary, uniquely named service
drop-in replaces the legacy artifact-verification startup command with that manager's full
verification. It keeps the ordinary update lock, queue validation, artifact hashing, service
health check and rollback path. The live checkout changes only through that updater.

For the installed Dev unit whose only `ExecStartPre` is the legacy verification command:

```sh
set -eu
umask 077
checked_commit=$(git rev-parse HEAD)
cargo build --locked --release -p dispatch-host
manager_target=$(cargo metadata --locked --no-deps --format-version 1 |
  python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
bootstrap=$(mktemp -d "${XDG_RUNTIME_DIR:?}/dispatch-support-bootstrap-XXXXXX")
install -m 700 "$manager_target/release/dispatch-host" "$bootstrap/dispatch-host"
unit_directory="$HOME/.config/systemd/user/dispatch-dev.service.d"
mkdir -p "$unit_directory"
dropin=$(mktemp --suffix=.conf "$unit_directory/99-ai-support-bootstrap-XXXXXX")
printf '[Service]\nExecStartPre=\nExecStartPre="%s" host dev --root %%h/dispatch-platform/dev --verify\n' \
  "$bootstrap/dispatch-host" > "$dropin"
systemctl --user daemon-reload
"$bootstrap/dispatch-host" host dev --root "$HOME/dispatch-platform/dev"
"$bootstrap/dispatch-host" host dev --root "$HOME/dispatch-platform/dev" --wait "$checked_commit"
"$bootstrap/dispatch-host" host dev --root "$HOME/dispatch-platform/dev" --install-management
"$bootstrap/dispatch-host" host dev --root "$HOME/dispatch-platform/dev" --verify-management
rm -- "$dropin"
systemctl --user daemon-reload
systemctl --user restart dispatch-dev.service
"$bootstrap/dispatch-host" host dev --root "$HOME/dispatch-platform/dev" --wait "$checked_commit"
rm -- "$bootstrap/dispatch-host"
rmdir -- "$bootstrap"
```

If the installed unit has other startup commands, retain them in the temporary override.
If checks are pending, rerun the ordinary update command after they pass. Keep the drop-in and
standalone manager until the new runtime is healthy and `--install-management` has installed
the complete verified management bundle, including both Python compatibility files. An
automatic binary refresh alone leaves the legacy Python guard in place. Only then remove the
specific drop-in, reload the unit, and restart through its original startup verification.
Remove the temporary standalone manager after that restart is healthy. The installer retains
the previous management bundle for rollback. Running `--install-management` before activation
would copy the old manager; never bypass the artifact allowlist or use an unverified candidate.

Production has the same compatibility transition, reserved for a separately authorized future
release rollout. Transfer the trusted standalone manager over authenticated SSH into a unique
private staging directory with mode `0700` and compare its SHA-256 with the local binary. The
temporary override must use full `host production --root ... --verify` for the actual installed
Production service. Run only the ordinary Production updater after the authorized release is
published. Once its runtime is healthy, install the complete management bundle before removing
that specific override and reloading the unit; restart through the original startup guard and
confirm health before removing the staged manager. Preserve unrelated unit overrides and the
retained previous management bundle. The updater still requires the immutable published
release, provenance, attestation, archive digest and source binding. This procedure publishes
no release and authorizes no Production change. Production never enables a worker.

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
Both trusted supervisors use the existing root-owned
`/usr/local/libexec/dispatch-dev/bwrap` when installed, otherwise `/usr/bin/bwrap`.
The dedicated Dev installation permits nested sandboxes under its existing AppArmor profile.
Host validation checks the selected executable and its parents; native verification carries
that exact executable into its sealed receipt and isolated configuration. The wrapper and
canonical BrowserOS release metadata are mounted together read-only for the inner process.
Only the isolated collector decrypts credentials. A shared
OS account lease excludes normal verification-account provider browsers while verification
runs; source credential rotation or disablement cancels the run. Temporary encrypted credentials are
deleted afterward. Changed collector output remains in private isolated state so the
reviewer can exercise the actual collected rows in a real browser. Collector logs and raw
provider records never become agent-tool or GitHub responses. Production's service logs reach
only the analyst of a Production ticket, through the reader below, limited to that ticket's DSP.

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

## Reading Production's server

The analyst of a Production ticket, past intake, can read Production's server with
`dispatch_production_server`: its health, each service's status, service logs and resources.
Production's `dispatch-reader` account has one SSH key, accepted only from Dev's Tailscale
address, whose forced command is `ops/scripts/production-reader.py`. It runs a request only if
it is one of those reads: no shell, files, database, configuration or forwarding. App logs keep
the ticket's DSP and platform events, never another DSP's or unsigned traffic, and drop each
request's client fingerprint and hashed account. The worker names the DSP, never the model. The
key, host and pinned host key stay outside the model's context. What the reader returns is
owner-private evidence, never quoted in a reporter's reply or a PR.

Setting it up once on Production, with the reader script from the checked commit:

```sh
sudo useradd --system --create-home --home-dir /var/lib/dispatch-reader \
  --shell /bin/sh --groups systemd-journal dispatch-reader
sudo usermod -p '*' dispatch-reader   # no password; only its key signs in
sudo install -o root -g root -m 0755 -D production-reader.py \
  /usr/local/libexec/dispatch-reader/production-reader
sudo install -o root -g root -m 0755 -d /var/lib/dispatch-reader/.ssh
printf 'from="<Dev Tailscale address>",restrict,command="/usr/local/libexec/dispatch-reader/production-reader --owner <service account> --health http://127.0.0.1:5180/api/health" %s\n' \
  "$(cat production-reader.pub)" |
  sudo install -o root -g root -m 0644 /dev/stdin /var/lib/dispatch-reader/.ssh/authorized_keys
```

On Dev the key pair and Production's pinned host key live beside the worker's tokens, and the
worker's configuration names them:

```json
"productionReader": {"host": "<Production Tailscale address>", "user": "dispatch-reader",
  "keyFile": "<runtime>/production-reader_ed25519", "knownHosts": "<runtime>/production-reader.known_hosts"}
```

Without `productionReader`, Production tickets are investigated as before, without the server.
