# Database

Stores, connections, the migration engine and each database's ledger, with the shared
databases' baselines. Every database gathers its numbered migrations from the owners that
declare them. Migrations only add, so the previous release keeps working on a migrated
database, and a shipped one never changes; `tests/backend/schema/` holds each schema's snapshot.
