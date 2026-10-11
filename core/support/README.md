# AI Support

A core capability available to every authenticated user. It is independent of DSP feature
switches. The account menu opens a corner panel for bug reports, feature requests, screenshots,
private ticket conversations and persistent notifications. Platform owners review the Support
inbox, ask Hermes private questions or message the reporter, then approve a particular proposal
version or decline it. Approval authorizes implementation, independent verification and merging
through the existing queue. Publishing a production release remains a separate owner action.

The same worker and versioned Hermes profiles handle Dev and Production. Environment, ticket
identity and deployed source are job context; profile instructions, skills and tools are shared,
except that a Production investigation can also read Production's server through its read-only
`dispatch-reader` account (`ops/support-worker.md`). The production server stores its own tickets
and exposes the scoped worker protocol. Hermes and the worker run on the development host. A queued job remains durable while the worker or provider
is unavailable. There are no support-specific daily, token, runtime or attempt quotas.

## Ownership

- `backend/` owns all support SQL and transitions: tickets, conversations, attachments,
  immutable proposals, owner approvals, jobs and leases, browser evidence, notifications and
  correlated GitHub feedback.
- `api/` owns routes, Rust wire types, generated TypeScript, reply validation and browser calls.
- `frontend/` owns the report panel, member tickets, notifications and owner inbox. The shell
  renders generic account action and indicator slots rather than importing this UI.
- `migrations/platform/0023_ai_support.sql` is additive on the existing platform database.
- `tests/` covers privacy, authorization, restart recovery, approval versions, verification and
  real member/owner browser journeys.
- `services/support-worker/` owns scheduling, the isolated Hermes roles and trusted browser,
  collection and GitHub adapters. Support state in the platform database is authoritative.

The server derives reporter name, DSP id and DSP name from the authenticated account. The form
captures no submission-page URL: the description identifies where a problem actually occurs.
A reporter sees only their own tickets in their own account directory, shared conversation and
report screenshots. Other DSP members cannot read them. Technical proposals, private agent
questions and verification evidence are owner-only. A worker identity cannot approve proposals.

## Workflow

Submission atomically stores a ticket and investigation job. Retries using the same request id
return the original ticket. Investigations browse first and analyze even when reproduction is
inconclusive. A structured, versioned proposal moves the ticket to `awaiting_approval`. Approval
queues implementation for that exact version and its acceptance criteria. A revised proposal
revokes old implementation authority before publication. Once a PR is published, its approval
remains immutable while GitHub can merge it. Owner questions can still receive private answers.

Jobs use 90-second leases with regular heartbeats and durable checkpoints. Crashes resume the
same job; an expired worker loses its authority. Waiting for provider assistance can be resumed
by the owner, and temporary infrastructure waits can retry automatically. Independent browser
evidence must cover every approved criterion at the exact PR head; a later failing observation
invalidates an earlier pass. The trusted worker publishes, corrects check failures or conflicts,
re-verifies and keeps working until GitHub confirms merge.

`merged` means the checked change reached main and is awaiting production release. It becomes
`resolved` only after a deployed production release contains the merge and its browser behavior
has been verified. Reporters can reopen a declined, merged or resolved ticket for a new analysis.

## Routes and configuration

Session routes are under `/api/support`; owner decisions are under `/api/platform/support`.
Dedicated service routes are under `/api/v1/support/worker`, with a bearer credential and no
browser cookie. Worker requests are lease-bound. GitHub event callbacks use a separate credential
and must match the registered ticket, approval version, branch, PR and exact head.

The app reads `DISPATCH_DEV_SUPPORT_WORKER_TOKEN` and `DISPATCH_DEV_SUPPORT_GITHUB_TOKEN` on
Dev, and `DISPATCH_PRODUCTION_SUPPORT_WORKER_TOKEN` and `DISPATCH_PRODUCTION_SUPPORT_GITHUB_TOKEN`
on Production. Each credential must contain at least 32 characters. Missing credentials disable
the corresponding adapter; reports still save and queue. Keys live in private host configuration,
never in profile instructions, the model context, browser tools or Git. Previews use isolated
fixtures and do not contact Hermes, GitHub or collection providers.

The investigation browser receives a dedicated read-only cookie tied to its active job and the
reporter's current account access. The server rejects writes, including public routes; opening a
DSP view skips its audit write. This identity cannot change account security, submit support
messages or use worker credentials. Screenshots are authenticated PNG, JPEG, WebP or GIF uploads
up to 20 MiB; staged uploads expire after a day if unused. Screenshot content is stored privately
and is never automatically posted in a public PR.
