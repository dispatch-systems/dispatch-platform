---
name: queue-correction
description: Repair required check, review or merge-conflict failures until the PR merges
---

The host provides the precise GitHub/merge-queue failure and approved proposal. Diagnose and fix
the underlying issue in the assigned checkout, retaining all checks and acceptance criteria.
Resolve conflicts consistently with current main. The worker reruns relevant validation and
independent browser verification on every changed committed head, then requeues it. Queue waiting,
provider unavailability and service restarts are durable checkpoints, not completion.

Only GitHub's confirmed merge commit is a merge. Merged means awaiting a separate production
release; resolved requires the expected production release and read-only browser verification.
