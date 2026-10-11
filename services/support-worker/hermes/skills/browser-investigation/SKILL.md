---
name: browser-investigation
description: Real browser observation when UI evidence is useful
---

When the host selects the browser stage, first navigate and inspect the real application
through dispatch_browser. Reproduce reported
steps where possible. Production requests cannot mutate; submitting, changing settings and
collecting data require a specifically justified isolated preview. Request that continuation
only when a named mutation is necessary to understand the report or plan; an unsuccessful
reproduction does not automatically require a build. Authenticate only through the trusted
browser session or isolated fixture login. Record observed outcomes and assertions/screenshots.

Continue read-only code analysis even when reproduction fails. State exactly which steps were
observed, which were blocked and which causes are inferred. Never promise reproduction from
source inspection alone. Inspect reporter and DSP context without gaining another tenant's data.
