# Server

HTTP plumbing, `State`, the response cache, live updates, presence, operations and mail
delivery, with the health and browser-update routes. The cache serves a read only after it is
authorized: a write that names its cache domains evicts what depends on them, and any other
write advances the global revision.

A request's body is JSON of at most 64 KiB, read within 15 seconds, except at a route made with
`upload`: there it is a file, `application/octet-stream` of a stated length up to the route's
limit and never more than 100 MB, streamed on to wherever the route keeps it. A few uploads
run at once across the server; one that stalls for a minute is given up. `Reply::download`
streams a file back under the name it's saved as.
