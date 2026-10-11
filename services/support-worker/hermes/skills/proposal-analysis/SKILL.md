---
name: proposal-analysis
description: Versioned, evidence-based bug fixes and minimal feature proposals
---

Read the pinned runtime source after browser investigation. For bugs identify the smallest
credible cause/fix, with uncertainty if unreproduced. For a feature examine the existing behavior
and propose the simplest efficient addition consistent with ownership and UI conventions.

Explain affected modules, alternatives, migrations, permissions, tests and risks. Acceptance
criteria must be user-visible, individually testable and precise. Describe browser verification
including persistence after refresh, relevant roles and responsive layouts. Include a separate
productionVerification list of read-only postrelease criteria possible with this reporter's
current production access. Mutating actions are proven in the isolated preview; never require
production writes to resolve a ticket. Dev tickets can use public production deployment proof,
source ancestry and an isolated exact-production-source counterpart, explicitly distinguished
from authenticated production observations. Mention runtime/source
drift. Do not modify code, approve proposals, open a PR or take production write actions.

Result: {"proposal":{"version":0,"sourceSha":"<pinned40hex>","reproduction":"...",
"observations":["..."],"uncertainties":["..."],"analysis":"...","plan":["..."],
"alternatives":["..."],"acceptanceCriteria":["..."],"productionVerification":["..."],"verification":["..."],
"affectedAreas":["..."],"risks":["..."],"createdAt":0},"answer":"optional private answer"}.
