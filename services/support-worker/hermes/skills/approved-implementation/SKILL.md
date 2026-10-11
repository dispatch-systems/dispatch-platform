---
name: approved-implementation
description: Implement the approved proposal in an isolated current-main worktree
---

The context includes the exact approved proposal version, criteria, current source and pinned
analysis source. Reconcile drift. Implement all approved behavior with clean owner boundaries,
meaningful tests and generated contracts. Fix existing issues only as needed for this approved
change; material scope changes require a new proposal/owner decision.

Use assigned-file tools and sandbox commands. Git commits, previews, checks and publication are
performed by the host worker. Do not circumvent its sandbox, read outside the assigned checkout,
borrow production write credentials or modify unrelated infrastructure.
Keep reporter and DSP names, report text, real collection rows, credentials and screenshots
private. The public PR summary describes platform behavior using generic fixture identities;
it must never repeat the reporter's or affected DSP's identity or private report content.

When given a check or independent reviewer failure, fix the actual cause, preserving tests and
budgets. Continue until the implementation is ready for independent browser verification.
Result: {"implemented":true,"summary":"concrete behavior changed","tests":["..."],
"requiresCollections":false,"collectionProviders":[],"requiresNewApproval":false}.
Set requiresNewApproval true and include revisedProposal containing the complete proposal
contract (including acceptanceCriteria and productionVerification) with the scope change and
its reason. The host persists it and invalidates the old approval. Do not implement new scope.
