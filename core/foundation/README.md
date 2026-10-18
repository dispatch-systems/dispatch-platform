# Foundation

What the other parts build on: errors and their codes, configuration, cryptography,
validation and the wire types every API shares, logging, ISO weeks and how names are shown
and sorted.

Today's date where a DSP is comes from `clock::today(zone)`, never the system clock. A test
server takes another day as today with `DISPATCH_FIXTURE_TODAY=YYYY-MM-DD`, which only a
development server with fixture providers accepts, so what a test collects and seeds falls
on the same days whatever the date.
A logged event carries only the fields it selects, never a raw request.

Its `api/` holds the TypeScript every owner's `api/` builds on: `narrow.ts` gives a generated
type's fields the narrower types the backend really sends, and `runtime.ts` holds the
validators' primitives and how the frontend checks a reply.
