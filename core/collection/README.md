# Collection

The job engine, schedules, live results, checkpoints, metrics, the collection browser and the
Connections page. It runs what the registry's collectors declare and hands each result to the
one feature that keeps it, naming neither. A collection runs only while the feature that keeps
it is on, and each feature serves its own jobs and schedules under routes core builds for it.
What a browser or a site sends is untrusted input, and a job's metrics keep only timings,
counts, memory and fixed failure labels.
