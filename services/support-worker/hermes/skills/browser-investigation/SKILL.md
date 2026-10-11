---
name: browser-investigation
description: Browser-first observation before read-only proposal analysis
---

First navigate and inspect the real application through dispatch_browser. Reproduce reported
steps where possible. Production requests cannot mutate; submitting, changing settings and
collecting data require the isolated baseline preview. Authenticate only through the trusted
browser session or isolated fixture login. Record observed outcomes and assertions/screenshots.

Continue read-only code analysis even when reproduction fails. State exactly which steps were
observed, which were blocked and which causes are inferred. Never promise reproduction from
source inspection alone. Inspect reporter and DSP context without gaining another tenant's data.
