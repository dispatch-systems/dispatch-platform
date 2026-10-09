# Collection

The job engine, schedules, live results, checkpoints, metrics, the collection browser and the
Connections page. It runs what the registry's collectors declare and hands each result to the
one feature that keeps it, naming neither. A collection runs only while the feature that keeps
it is on, and each feature serves its own jobs and schedules under routes core builds for it.
What a browser or a site sends is untrusted input, and a job's metrics keep only timings,
counts, memory and fixed failure labels.

A page follows its collections live with `useCollectionUpdates()` (`frontend/live-collection.ts`):
one waiting request on `/api/dsp/collection-updates`, which the permissions features list in
`live` may make, wakes as a run ends or a collector shows results partway, and its feature's
`collected` and `collection` cache rules say which reads that refreshes. A member hears only of
the collections kept by features whose `live` permissions it holds, so a hint, such as the
employee code a change names, stays with its feature's readers. A job's status isn't
announced, so a page shows a run's progress by reading its status while it runs.
