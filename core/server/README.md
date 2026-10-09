# Server

HTTP plumbing, `State`, the response cache, live updates, presence, operations and mail
delivery, with the health and browser-update routes. The cache serves a read only after it is
authorized: a write that names its cache domains evicts what depends on them, and any other
write advances the global revision. Live updates are wakeups: collection progress has its hub,
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

The server answers at three kinds of address, each its own origin: the platform owner's admin
(`DISPATCH_ORIGIN`), the invite page where a new DSP's first owner sets it up
(`DISPATCH_INVITE_ORIGIN`), and each DSP's own (`DISPATCH_DSP_ORIGIN`, its short code in place
of `{code}`). A deployed server names all three; in development, the last two are this server
under `localhost`'s names, `invite.localhost` and `<code>.localhost` at its port. A request's
`Host` decides which it came to, and its `Input` carries that `Site`; any other host is
refused, and a write must come from the pages of the address it came to. The agent API, its
sign-in and the pages outside services send the browser back to are the admin's alone.
