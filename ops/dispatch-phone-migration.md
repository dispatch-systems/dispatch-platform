# Dispatch Phone migration

Dispatch Phone replaces the Communications name. The migration preserves the same DSPs,
conversations, messages, attachments, settings, access and paired phones. This plan separates
code changes from releases: merging a bridge is not evidence that Production has installed it,
and a unit test is not evidence of a phone updating or of a successful production migration.

## Sequence

1. **Address and name bridge.** Change the existing visible wording and accept the new page
   and API addresses beside the existing ones. Both registrations share the same handlers,
   authorization, tenant checks, upload limits and invalidation policies. Preserve conversation
   and group parameters on either spelling. Continue emitting the old addresses until this
   bridge is an installed rollback target: its predecessor cannot open a new page address.
   Keep every stored identity and byte location unchanged. Existing Relay versions continue
   using their original address for messaging and updates.
2. **Storage compatibility bridge.** Before changing a durable identity, make the current
   runtime able to read and write both the existing and proposed storage layouts. Give the
   feature an explicit layout version in each DSP's storage; select all of its table names
   through that layout, with fixed reviewed identifiers rather than rewriting arbitrary SQL.
   Add generic support where core owns persistence: migration-owner aliases, feature and
   permission identifier aliases, section storage keys and integration identities. The bridge
   keeps writing the existing layout and must also work on a fixture of the proposed layout.
   It must preserve a single authoritative record when either spelling is received.
3. **Install the completed bridge.** Its own checks, reviews and Dev installation must pass.
   Exercise an already-paired Dev Relay through an update and a dashboard reply. A production
   release requires the user's specific approval. Do not activate the storage migration until
   the completed bridge is the installed, verified rollback target in that environment.
4. **Migrate durable names and active source.** Switch emitted page/API/notification/initials
   addresses to `dispatch-phone` once the address bridge is the rollback target. Rename the
   feature folder/crate/modules and active symbols to `dispatch_phone` / Dispatch Phone.
   Preserve the old migration ledger as immutable history and recognize its owner through a
   declared alias. Migrate per-DSP database names and configuration in transactions, leaving
   row IDs and values unchanged. Atomically advance the layout version only after its database
   changes succeed. The completed bridge must read and update this new layout after rollback.
   Relay may begin using the new protocol address once its rollback server supports it.
5. **Migrate file locations separately.** Inventory the old disk and R2 namespaces, copy bytes
   to the new namespace, verify each SHA-256 and persist restartable progress. Readers try the
   new location and then the legacy one until verified complete. Uploads, downloads, upkeep,
   cleanup, backups and tenant deletion must use the same transition state, including writes
   admitted before a phase changes. Keep the original bytes through the rollback window. Never
   combine a metadata switch with an unverified bulk move or let cleanup treat legacy bytes as
   unreferenced while migration is active.
6. **Retire old storage in a later release.** Removal requires an installed rollback target
   that no longer needs the old storage, successful restore and rollback rehearsals, and a
   reviewed inventory of exactly what would be deleted. Keep old links and Relay protocol
   aliases for offline phones; their removal is a separate decision. Historical migrations,
   audit facts and compatibility declarations can retain the former name.

The address bridge is implemented. The first storage compatibility piece recognizes both
migration-owner names without rewriting receipts or replaying applied steps. It still records
new steps under `communications`. This is not the completed storage bridge: mutable settings,
permissions, integrations, table layouts and bytes still need their own compatibility work.
Before renaming the feature folder, teach the shipped-source history rule to resolve declared
lineage while preserving its existing digests and history entries. No storage rename is safe
to activate on the strength of migration-owner compatibility alone.

Settings compatibility is the next piece: section IDs and raw setting keys accept explicit
aliases, preserve member/DSP revisions and payloads, and rekey one record per family within
its tenant. Conflicting records are preserved and stop normalization. Browser section IDs are
normalized before cache insertion. Feature/permission/integration identities, Settings page
addresses and physical storage layouts remain separate unfinished gates.

## Identity inventory

| Surface                     | Existing identity                                                                                                     | Proposed active identity and preservation                                                                                                               |
| --------------------------- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Feature and GIF part        | `communications`, `communications.gifs`                                                                               | `dispatch_phone`, `dispatch_phone.gifs`; preserve enabled, shown, beta, actor and timestamps for every DSP, including disabled and hidden states        |
| Role permission             | `communications.use`                                                                                                  | `dispatch_phone.use`; preserve grants and revocations; existing `phones.manage` needs an explicit mapping if renamed                                    |
| Member display section      | `communications.display`, `section:communications.display:<user>`                                                     | `dispatch_phone.display`; preserve every member's values and revision                                                                                   |
| Quick replies               | `communications.quick_replies`                                                                                        | `dispatch_phone.quick_replies`; preserve ordering, text and revision                                                                                    |
| Slack integration           | `communications.messages`                                                                                             | `dispatch_phone.messages`; preserve on/off state and its original on-since timestamp in every workspace                                                 |
| Migration ledger            | owner `communications`, IDs 1 onward                                                                                  | Recognize the original owner and never replay or rewrite shipped migrations because a crate or folder changed                                           |
| Tables and indexes/triggers | `communications_*` and adopted `phones_*`                                                                             | Explicit per-object map; preserve rows, foreign keys, indexes, triggers and pending work; the bridge understands both layouts                           |
| Files                       | DSP `data/communications`, `cache/communications`, R2 `dsps/<dsp>/communications/<sha256>`; adopted phone equivalents | New owner namespace selected by migration state; preserve original bytes and tenant ownership until copies are verified                                 |
| Page and dashboard API      | `communications`, `/api/dsp/communications`                                                                           | `dispatch-phone`, `/api/dsp/dispatch-phone`; preserve old links and their query parameters                                                              |
| Phone protocol              | `/api/communications/phone`                                                                                           | `/api/dispatch-phone/phone`; existing keys, phone IDs, client IDs and attempt IDs stay unchanged; old update URLs continue working                      |
| Browser state               | `dispatch-communications-*` session/local storage and coordination keys                                               | Migrate drafts and preferences on the client, per DSP/member/tab; preserve notification deduplication and shared push ownership across old and new tabs |
| Pushes, errors and history  | Old page URLs, wire error codes and audit action prefixes                                                             | New links for new notifications; keep old links, old-client refusals and historical audit facts understandable                                          |
| Conversation cursors        | `communications-window-v1` authenticated context                                                                      | Keep issued tokens readable through the bridge or deliberately reopen the same position; no silent history loss                                         |

The existing `retired_identifiers` helper is insufficient for this migration: its feature copy
does not copy `shown` and `beta`, and its destructive rewrites do not supply rollback readers
for settings, integrations, browser state, attachment locations or migration ownership. Do not
declare the feature's old identity retired merely to make the renamed build start.

## Required evidence

- Use populated fictional DSP databases with distinct enabled/disabled and Shown/Beta/Hidden
  states, different role grants, member display values, quick replies, connected workspaces and
  an intentionally disabled integration. Prove a tenant never inherits another tenant's state.
- Preserve all message, contact, group, file and phone IDs, timestamps, ordering, waiting/Done
  state, queued sends, delivery attempts, pending uploads and audit records. Compare attachment
  hashes, not just metadata or counts.
- Run upgrade twice, restart after each phase, resume an interrupted file copy, and exercise
  writes during transition. A repeated migration must neither resend nor duplicate a message.
- Write through both API spellings and read through the other; verify refusal, tenant
  isolation, upload and cache behavior. Old initials and attachment URLs remain readable.
- Open old conversation/group links with their query parameters, preserved browser drafts and
  preferences, and verify notifications do not duplicate across old/new tabs.
- Rehearse rollback using the previous actual release binary against the migrated fixture,
  then write using that runtime, upgrade again and verify that those writes survive.
- Exercise old Relay with the new server and new Relay with the bridge server, including the
  update download path and an existing pairing. Real carrier/device proof is reported apart
  from emulator, API and unit-test evidence.
- Before an approved production release, confirm a recoverable backup and the installed
  rollback version. Production access, release activation and any later deletion require the
  user's specific authorization.
