# Server

HTTP plumbing, `State`, the response cache, live updates, presence, operations and mail
delivery, with the health and browser-update routes. The cache serves a read only after it is
authorized: a write that names its cache domains evicts what depends on them, a write route
on a DSP's own access evicts that DSP's cached reads and those every DSP shares, and any other
write advances the global revision, evicting every DSP's. Live updates are wakeups: collection progress has its hub,
and each feature's own channel is a `Topic` of `state.topics`, named by the feature, so a
request waiting on one wakes when a write there notifies it and reads what changed from its
feature's storage.

A request's body is JSON of at most 64 KiB, read within 15 seconds, except at a route made with
`upload`: there it is a file, `application/octet-stream` of a stated length up to the route's
limit and never more than 100 MB, streamed on to wherever the route keeps it. A few uploads
run at once across the server; one that stalls for a minute is given up. `Reply::download`
streams a file back under the name it's saved as, and `Reply::picture` answers one the page
shows, which the browser keeps for a week for that view of the DSP: its address names the
version, so a changed picture comes at a new one.

`route.also_at("/old/address")` registers a compatibility address beside a route's canonical
one. Flatten its two registrations into the owner's routes: both share the handler, access
gate, upload limit and write policy, and each parses its own path and parameters. Keep both in
the route inventory while clients use either address.

The built dashboard is served from memory, loaded as the server starts: its page at `/`, its
files at `/assets/`, and at `/service-worker.js` the worker that shows what the server pushes to a
browser, at the root so it serves every page of its address.

The server answers at three kinds of address, each its own origin: the platform owner's admin
(`DISPATCH_ORIGIN`), the invite page where a new DSP's first owner sets it up
(`DISPATCH_INVITE_ORIGIN`), and each DSP's own (`DISPATCH_DSP_ORIGIN`, its short code in place
of `{code}`). A deployed server names all three; in development, the last two are this server
under `localhost`'s names, `invite.localhost` and `<code>.localhost` at its port. A request's
`Host` decides which it came to, and its `Input` carries that `Site`; any other host is
refused, and a write must come from the pages of the address it came to. The agent API, its
sign-in and the pages outside services send the browser back to are the admin's alone.

Database work uses four shared workers and a 64-entry queue. Routine polling and background
work first wait in a separate bounded queue of 56 callers for their three-worker share, so
waiting for that share does not occupy the shared queue, and background work holds at most two
workers at once. Both queues together retain at most 120 callers; this does not bound idle HTTP
waits or every task the server retains. Normal worker waits share one two-second deadline;
patient work waits up to a minute. A refusal is `platform_busy`, and the request's log names
the limiter that refused it. Started blocking work retains its admission until it finishes,
including after its caller disconnects.

Validated live-page writes use the per-DSP staging lane alongside foreground WAL readers.
After the transaction, the blocking worker briefly takes the exclusive transition to drain
readers and evict LIVE caches before returning and notifying readers, even if its caller
cancelled. Final publication and tenant lifecycle changes still use exclusive transitions.
