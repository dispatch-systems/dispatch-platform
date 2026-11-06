# Database

Stores, connections, the migration engine and each database's ledger, with the shared
databases' baselines. Every database gathers its numbered migrations from the owners that
declare them. Migrations only add, so the previous release keeps working on a migrated
database, and a shipped one never changes; `tests/backend/schema/` holds each schema's snapshot.

The object store (`objects`) is a bucket of Cloudflare R2, spoken to as S3 is, where pictures
and files are kept off the server's disk: each key `dsps/<dsp>/<owner>/<sha256>`, made by the
server of a DSP's ID it checked, an owner's name and a SHA-256, so one DSP never reaches
another's, and `delete_dsp` lets go of one DSP's alone. The environment names it with
`R2_ACCOUNT_ID`, `R2_BUCKET`, `R2_ACCESS_KEY_ID` and `R2_SECRET_ACCESS_KEY`, under
`DISPATCH_DEV_` on Dev and its previews or `DISPATCH_PRODUCTION_` on Production: with none of
them, there is none and every byte stays on disk; with only some, the server refuses to start.
`R2_ONLY`, under the same prefix, `1` once the release a rollback would return to reads the
object store, lets what it keeps leave the server's disk (`ObjectStore::only`); until then each
byte stays on disk too, so a release that reads only the disk finds it. Set without the bucket,
or to anything but `0` or `1`, the server refuses to start.
