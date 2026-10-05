//! Core's integration tests, one program for all of them, each file a module named for it, from
//! every part that has them: one program links core once, where a program per file linked it
//! seven times. `npm run test:core -- <part>` runs a part's modules by name.
#[path = "../../../../accounts/tests/backend/integration/accounts.rs"]
mod accounts;
#[path = "../../../../platform_owner/tests/backend/integration/audit.rs"]
mod audit;
#[path = "../../../../server/tests/backend/integration/browser_update.rs"]
mod browser_update;
#[path = "../../../../collection/tests/backend/integration/egress.rs"]
mod egress;
#[path = "../../../../collection/tests/backend/integration/jobs.rs"]
mod jobs;
#[path = "../../../../server/tests/backend/integration/mail.rs"]
mod mail;
mod storage;
