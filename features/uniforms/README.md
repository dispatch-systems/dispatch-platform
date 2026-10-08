# Uniform Inventory

A DSP's uniform catalog and its stock counts, with their change journal, in one database. Stock
writes are deltas and catalog edits never set a count, so concurrent adjustments add up. Its
page follows other sessions' changes through a long poll.

- **Switch:** `uniforms`, off for every DSP until the platform owner switches it on. It needs
  no connection.
- **Permissions:** `uniforms.view`, to see the inventory and its history. Under it, two finer
  parts, each granting View too: `uniforms.adjust`, to add to or take from a uniform's count,
  and `uniforms.manage`, to add and edit uniforms, their categories, fits and sizes, and to
  archive them. Manage doesn't include Adjust: counting stock and shaping the catalog are
  separate jobs. The demo DSPs give Managers View and Adjust, and Members View.
- **API:** behind `uniforms.view`, `GET /api/dsp/uniforms` (the catalog with every count),
  `GET /api/dsp/uniforms/history` (the journal, 50 events a page, newest first) and
  `GET /api/dsp/uniforms/updates?after=<revision>`, which answers at once when anything changed
  since that revision, or after up to 20 seconds of waiting. Behind `uniforms.manage`:
  `POST /api/dsp/uniforms/initialize` (the starter catalog: polos, shorts, pants, a spare vest
  and jackets in their sizes, once, before anything else), `POST /api/dsp/uniforms` (a new
  one), `POST …/{id}` (an edit) and `POST …/{id}/archive`. Behind `uniforms.adjust`,
  `POST /api/dsp/uniforms/stock/{id}`: one up or one down on a variant's count, kept between 0
  and 1,000,000 (`uniform_out_of_stock`, `uniform_quantity_limit`). Each carries a request id,
  so a retried one counts once.
- **Live updates:** every write bumps the inventory's revision and wakes the `uniforms` live
  channel, so each open page fetches only what changed: the counts adjusted since its revision,
  or the whole catalog after an edit or a long absence. A long poll holds no database connection while it waits, and
  checks access again before it answers.
- **History:** each change is journaled in the DSP's own database (`uniform_events`), with who
  made it and when: the catalog initialized, a uniform created, updated or archived, a count
  adjusted. A platform owner's change reads as "Platform support". The journal is the
  feature's own; nothing it does reaches the platform's audit log.
- **Storage:** the `uniform_inventory`, `uniforms`, `uniform_variants` and `uniform_events`
  tables of each DSP's database, created by its DSP migration 2. Switching the feature off
  deletes none of them.
