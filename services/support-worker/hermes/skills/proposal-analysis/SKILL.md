---
name: proposal-analysis
description: Versioned, evidence-based bug fixes and minimal feature proposals
---

The host selects a named stage. Tool-free intake acknowledges genuine test/greeting/thanks
messages and answers supported one-off questions; it never investigates or invents a proposal.
Source analysis has no browser tool. Browser analysis has a separately authorized read-only
live session or a specifically requested isolated preview. Follow the stage's requested JSON
contract; do not demand a browser or build merely because a report exists.

Read the pinned runtime source when it is useful. For bugs identify the smallest
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
drift. Vague real reports deserve best-effort source/UI investigation. If the intended issue or
feature outcome remains unclear after that attempt, report unclear_bug/unclear_feature with
specific findings and questions. Unreproduced behavior alone is not an unclear report: publish
a reasoned proposal when the intended issue is understood. Infrastructure failures remain
uncertainty and may require waiting; never disguise them as successful verification.
Do not modify code, approve proposals, open a PR or take production write actions.

The proposal value in the selected analysis result contract is:
{"version":0,"sourceSha":"<pinned40hex>","reproduction":"...",
"observations":["..."],"uncertainties":["..."],"analysis":"...","plan":["..."],
"alternatives":["..."],"acceptanceCriteria":["..."],"productionVerification":["..."],"verification":["..."],
"affectedAreas":["..."],"risks":["..."],"createdAt":0}.
