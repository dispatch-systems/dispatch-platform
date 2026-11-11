# Release verification on Dev

The release coordinator keeps source, CI, attestation, smoke, draft and installed-runtime
verification. A release with migrations additionally requires a fresh real Production backup
that has restored, passed every database integrity check, and started with the exact prepared
candidate on physical `dispatch-dev`. Production only captures/exports the backup and supplies
operational readiness and install observations. No synthetic restore or startup test runs there.

## Before authorizing a release

The owner separately approves the concrete Production readiness/capture commands and service
pause, real backup/key/config transfer and use on Dev, and publication/install observations.
A backup contains private data and the platform encryption key. Captured configuration may
contain provider secrets. Approval for a code change is not approval for these operations.
Confirm the deployed layout before proposing commands: the capture/readiness implementation
still targets the existing user services and public checkout. This change does not migrate
services or modify nightly backup drills, retention or runtime-only rollback.

Confirm Dev capacity and Bubblewrap namespace support without changing permissions. The tool
refuses any execution host other than the kernel hostname `dispatch-dev`, missing/unusable
`/usr/bin/bwrap`, or unfinished private staging. There is no bypass or imported-proof option.
Any installation or permission change requires separate approval. CI's host tests use synthetic
fixtures and the existing support-sandbox setup.

## Capture and verification

With the operational approvals in place, the existing `publish --backup` flow captures a
backup once Production passes readiness. It resumes the service and updater before export.
Standalone `backup` now requires that version's complete verified prepared candidate; it
refuses before capture if none exists. It never falls back to Production's installed backend
for a restore drill.

The coordinator exports the captured backup and config into a private directory under
`/data/dispatch-release-verification/` on Dev. Extracted input is limited to 8 GiB and 100,000
members; transport allows that content plus bounded tar headers. Only unique safe regular files/directories are allowed. The backup manifest's file
hashes and sizes must match, including the original 32-byte platform key; a missing key refuses
rather than generating a substitute. The captured startup env must still match Production's current
`config/platform.env` at every export. Other required external files are not mounted: missing
configuration refuses startup rather than using Dev's live configuration.

The verified candidate is unpacked locally. Its real `restore` command runs in Bubblewrap with
separate user, mount, PID, IPC, UTS and network namespaces, no host home/live mounts, a readonly
runtime and input, and a cleared environment. Every restored SQLite database must pass its
full integrity check and at least one database must exist. A second independently restored
copy runs the same candidate's real `upgrade-check` under the captured Production/native
configuration with only state and artifact paths redirected. This entrypoint starts no
listener, job, reader, provider worker or mailer. Network isolation also denies egress if
startup ever attempts it.

The disposable working filesystem is a private disk-backed directory on Dev's `/data`
filesystem, isolated from the live service's root disk. Preflight requires 24 GiB free:
two 8 GiB copies plus an 8 GiB reserve. The archive is removed after extraction and the
integrity-checked restore is removed before the independent upgrade restore. Sandbox `/tmp`
is limited to 64 MiB, individual backend files to 8 GiB and address space to 4 GiB.
Export has a 30-minute timeout and the sandbox a one-hour timeout. The disk workspace has
no aggregate quota: the free-space preflight and per-file bound cannot prevent migration
growth or concurrent builds from exhausting `/data`. Insufficient capacity refuses;
these bounds do not promise that every Production dataset fits. No raw backend/config output
leaves the sandbox. Startup errors are reported as counts, without private event fields.

## Evidence, retry and custody

Format-2 backup evidence binds executor, release version, exact commit/runtime/archive,
original `backup.json` capture time, complete content inventory and configuration hashes,
file/DB counts and completed cleanup. The startup receipt binds that evidence and its report.
Old Production receipts cannot satisfy the Dev gate. Reuse exports and checks the inventory
again; a changed backup, candidate or startup config invalidates the proof.

After source/CI/app checks, startup and the last backup export, Production is checked again.
Only local gates follow that final readiness observation: candidate bytes, matching startup
proof, original capture age and the observed current startup-config digest. A new collection
or changed startup configuration cannot hide behind a previously passing Dev proof. Six hours remains the limit; transfer or verification never refreshes the clock.
Future or invalid timestamps refuse.

Successful and ordinary failed checks remove their private Dev staging before evidence can
pass. An interrupted coordinator or failed cleanup leaves private custody and blocks retry.
The tool names the staging directory; inspect/list it and obtain approval for deletion before
cleaning real data, keys or configuration. Do not fabricate receipts or copy passing state.
Runtime installation attestation, activation receipts and rollback gates remain unchanged.
Actual rollback and any data restore require separate owner approval.

The release-tool change also needs the usual separately authorized future dry run after it
ships. A draft/package smoke check does not replace the real-backup Dev rehearsal. Exact
artifact/data/config checks retain startup semantics but cannot prove identical machine
performance, feature regression freedom or destructive data rollback compatibility.
