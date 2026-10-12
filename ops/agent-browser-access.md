# Agent browser access

Use a short-lived, read-only observation session to inspect the live Dev or Production
dashboard. The reusable authority is this machine's existing local/SSH operator access;
there is no automation password or permanent owner cookie. Development previews continue
to use the automatic sign-in link from `dispatchdev preview <name>`.

From the Dispatch workspace, request a credential into the current change's private
scratch folder:

```sh
dispatchdev browser dev --out /tmp/dispatch-<name>/browser-dev.json
dispatchdev browser production --out /tmp/dispatch-<name>/browser-production.json
```

The output file must be a new absolute path; choose a new filename when renewing. It is
written with mode `0600`. The command prints only the origin, deadline and read-only status.
Production issuance runs the installed backend through `ssh dispatch-production`; its
platform signing key stays on that host. Both environments must run an installed build that includes
`dispatch-backend observation-session` and `/api/auth/observation`.

Open an Incognito T3 browser tab at the returned origin. Redeem the file's `token` with a
same-origin `POST /api/auth/observation`, JSON body `{ "token": "<credential>" }`, through
`preview_evaluate`, then reload. Check `GET /api/session` reports `readOnly: true` before
continuing. Keep the credential out of URLs, screenshots, command arguments and public logs.
Do not print the credential file. Delete it after the browser has accepted it.

The server sets a separate `HttpOnly`, host-bound, `SameSite=Strict` cookie, secure whenever
the configured origin uses HTTPS. Every request checks the signed origin and environment,
the original 15-minute deadline, and the current owner's active status and account version.
Changing that account's password/version, disabling it, removing its owner access, or rotating
the platform key invalidates observation access. To continue after expiry, mint another
credential and redeem it; no user sign-in is needed.

Observation access permits dashboard reads and opening a DSP view. The server rejects
mutations even on public routes and GET routes that perform writes. It grants no recent
verification authority and does not record a human's DSP visit. A malformed, duplicate or
expired observation cookie fails closed instead of using an ordinary owner cookie.

The read-only guard accepts routes declared as database reads or in-memory reads. Async
routes are denied unless they are the observation exchange or opening a DSP view. Some
external-provider pages, file downloads and live streams therefore remain unavailable in
observation access; a GET method alone does not prove that its handler cannot write.

These signed credentials can be replayed until their fixed expiry. They are not one-use
links, and there is no per-credential revocation record. Always use an Incognito tab so the
observation session never changes the human's saved browser profile. The dashboard labels
this access as read-only.

Browser observation does not authorize a Production change. Use the normal task approval
and release workflow for writes, and use a fixture preview for testing interactions that
change data.
