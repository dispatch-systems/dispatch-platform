# Platform owner

The platform owner's dashboard: DSPs with their feature switches and the eye that hides one
from a DSP's members, New Clients, Diagnostics, mail, the audit
log and the owner's own Settings page. The Agents page is the MCP's, in `mcp/`. It shows what features and collectors
contribute (switches, collection labels, audit wording) from the generated
catalog and their manifests' slots, and never names one.

## New Clients

The requests for a demo the public site's form sends (`backend/demo_requests.rs`), which the
platform owners are emailed and keep track of: New, Contacted, Demo booked, Signed up or Not a
fit, as an inbox or a board. The count beside the page's name is those still New.

The site sends each to `POST /demo-requests` at the platform owner's address, as JSON (`id`, its
own ID for the request, `name`, `email`, `dsp` and `message`), with `x-dispatch-timestamp`, the
time in seconds, and `x-dispatch-signature`, `v1=` and the hex HMAC-SHA256 of
`v1:<timestamp>:<body>` under the secret the two share: `DEMO_REQUESTS_SECRET`, at least 32
characters, under `DISPATCH_DEV_` on Dev and its previews or `DISPATCH_PRODUCTION_` on
Production. A request signed more than five minutes ago is refused, one sent again under the
same `id` is kept once, and at most 120 are kept in an hour. With no secret set the address
answers as if it weren't there, except with fixture providers, where a fixed one
(`FIXTURE_SECRET`) signs the tests' requests.

It stores, in the platform's database, `demo_requests`: who asked, their address, their DSP's
name, what they wrote, when, and where the owner stands with it.
