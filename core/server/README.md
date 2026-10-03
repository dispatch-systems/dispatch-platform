# Server

HTTP plumbing, `State`, the response cache, live updates, presence, operations and mail
delivery, with the health and browser-update routes. The cache serves a read only after it is
authorized: a write that names its cache domains evicts what depends on them, and any other
write advances the global revision.
