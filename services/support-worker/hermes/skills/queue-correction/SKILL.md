---
name: queue-correction
description: Repair required check, review or merge-conflict failures until the PR merges
---

The host provides the precise failure of the PR's required checks, or its merge conflict, and the
approved proposal. Diagnose and fix the underlying issue in the assigned checkout, retaining all
checks and acceptance criteria. Resolve conflicts consistently with current main. The worker
reruns relevant validation and independent browser verification on every changed committed head,
then publishes it, and GitHub merges it once its checks pass. Waiting on checks, provider
unavailability and service restarts are durable checkpoints, not completion.

Only GitHub's confirmed merge commit is a merge. Merged means awaiting a separate production
release; resolved requires the expected production release and read-only browser verification.
