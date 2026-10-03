# Foundation

What the other parts build on: errors and their codes, configuration, cryptography,
validation and the wire types every API shares, logging, ISO weeks and how names are compared.
A logged event carries only the fields it selects, never a raw request.

Its `api/` holds the TypeScript every owner's `api/` builds on: `narrow.ts` gives a generated
type's fields the narrower types the backend really sends, and `runtime.ts` holds the
validators' primitives and how the frontend checks a reply.
