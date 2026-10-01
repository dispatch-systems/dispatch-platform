# DVIC collection

Backend collection through the existing Cortex connection. Enable the DSP's `dvic`
feature and configure its station before collecting. The dashboard's DVIC page has a
Day tab (headline numbers, repeat drivers, and the day's records by vehicle type) and
a Week tab (drivers by day, showing the shortest inspection or the count), with week
navigation, driver/VIN search, vehicle filters, and inspection details. Bands state the
share of the minimum reached (75%+, 35–75%, under 35%) without a verdict. Viewing,
manual sync, and schedule management use the separate permissions below. Sync now
checks recent publication weeks; historical backfills remain available through the API.
The page reads every cursor page before displaying counts, and keeps source wall times
unchanged.
CV/CDV inspections shorter than 90 seconds and SV inspections shorter than 300 seconds
are exceptions. Exactly 90/300 seconds meets the minimum. Unknown vehicle types fail
validation rather than guessing a threshold.

## API

All endpoints use the existing authenticated DSP context and CSRF rules.

| Endpoint                                                      | Permission     | Purpose                                                                                       |
| ------------------------------------------------------------- | -------------- | --------------------------------------------------------------------------------------------- |
| `GET /api/dsp/dvic/status`                                    | `dvic.view`    | Recent jobs, report inventory, checked weeks, exception count                                 |
| `GET /api/dsp/dvic/inspections?from=2026-09-01&to=2026-09-30` | `dvic.view`    | Exceptions by actual inspection date; optional `driver`, `limit` (1–500), `after` cursor      |
| `POST /api/dsp/dvic/collect`                                  | `dvic.collect` | Manual sync with `requestId`; optional ending publication `week` and number of `weeks` (1–26) |
| `POST /api/dsp/dvic/jobs/{id}/cancel`                         | `dvic.collect` | Cancel this DSP's DVIC job                                                                    |
| `GET/POST /api/dsp/dvic/schedules`                            | `dvic.manage`  | List/create DVIC schedules                                                                    |
| `POST /api/dsp/dvic/schedules/{id}`                           | `dvic.manage`  | Edit a schedule with its current `revision`                                                   |
| `POST /api/dsp/dvic/schedules/{id}/enabled`                   | `dvic.manage`  | Pause/resume with `enabled` and `revision`                                                    |
| `POST /api/dsp/dvic/schedules/{id}/remove`                    | `dvic.manage`  | Delete with `revision`                                                                        |
| `POST /api/dsp/dvic/schedules/preview`                        | `dvic.manage`  | Preview cadence in the DSP's timezone                                                         |

Manual sync defaults to the current and previous publication week; an explicit week
alone selects that week. A request returns one queued job, and replaying its requestId
is idempotent. Schedules use the existing daily/interval scheduler, timezone handling,
revision checks, overlap prevention, cancellation, and retry/backoff policy.

Example daily schedule body:

```json
{
  "name": "Daily DVIC",
  "collection": "dvic",
  "cadence": "daily",
  "intervalMinutes": null,
  "localTime": "18:00",
  "enabled": true
}
```

No schedule is created automatically. A scheduled run checks current/previous
publication weeks and at most two older weeks in a rolling 26-week lookback. It
prioritizes never-checked weeks, then least recently checked weeks older than seven
days. Missing/empty reports stay eligible for later checks. Manual sync can target
older publication weeks explicitly. DVIC schedules are listed under their own API
until the dashboard supports them; they remain independent of timecard switches.

## Dates, storage, and efficiency

Amazon's selector labels Sunday–Saturday, while the observed report publication
weeks are ISO Monday–Sunday. Workbooks labelled `last7days` contained eight inspection
dates. The collector discovers reports by publication week and takes inspection dates
from the workbook rows; it never clips rows to the selector or a presumed seven days.
Source timestamps have no timezone offset and are retained as supplied.

One authenticated browser session discovers the DSP/company/station and API. Plain
HTTP lists reports; a separate cookie-free client reads the exact allow-listed report
host, four downloads at a time. Stable source paths and SHA-256 revisions are stored;
pre-signed URLs and their signatures are not. ETags permit 304 checks without reading
or parsing unchanged files. Workbook bytes, expanded ZIP contents, cells, and rows
are bounded. Scope, columns, timestamps, and duration are checked before publication.

`dsps/<id>/data/dvic/dvic.sqlite` stores report metadata, normalized source revisions,
canonical inspections, checked publication weeks, and collection runs. Publication is
atomic across the whole batch and can be retried after interruption. Natural identity
uses company, DSP, station, driver ID, VIN, inspection type, and normalized start time;
a name change cannot duplicate an inspection. The newest report observation wins;
older backfills cannot overwrite it. Explicit corrected observations meeting the
minimum are retained as non-exceptions so old reports cannot resurrect them. A row
missing from a rolling exception report does not delete history. These reports do not
establish the total number of inspections or a compliance percentage.

## Hidden drivers

An operator can keep drivers out of a DSP's DVIC data. `dvic_hidden_drivers` in the DSP's
`dvic.sqlite` lists their transporter IDs; publication drops their rows before writing any
report copy, count or inspection, so the database never holds them. Nothing in the
dashboard or the API reads or changes the list. Only the platform operator does, on the
host, with the backend binary and the service's environment:

```
dispatch-backend dvic-hidden <dsp_id>
dispatch-backend dvic-hide <dsp_id> <transporter_id> "<note>"
dispatch-backend dvic-unhide <dsp_id> <transporter_id>
```

Hiding also deletes the driver's stored inspections and removes their rows from every stored
report copy, recounting those reports, in one transaction. Unhiding lets their later reports
in and restores nothing. The commands change only that DSP's DVIC database, which SQLite
serializes with the running server's writes, so they need no stopped service; the server
must already run a release with migration 2. A page that shows fewer inspections than
Amazon's report may have a hidden driver: check `dvic-hidden` first.

## Release order

The `feat/dvic-schema` commit widens the jobs/schedules CHECK constraints without
queueing the new values. Release that preparation first. `feat/dvic-collection` adds
the collector on top and must ship in a subsequent release, so the rollback target
already accepts DVIC job/schedule values. The feature is off by default.

## Verification

A native collector run against a private copy of Dev data read 14 real workbooks
for publication weeks 38–39: 561 source rows became 103 unique short inspections
across 45 drivers. Collection took 6.9 seconds (9.4 seconds including authentication
and publication). Repeating it returned 14 HTTP 304 results, downloaded/parsed zero
workbooks, and retained exactly 103 inspections and 14 source revisions. The private
service was stopped and its data copy deleted afterward. No live DSP data changed.

Automated coverage includes threshold boundaries, overlapping files, corrected names
and durations, older backfills, atomic rollback, empty reports, pagination, station
isolation, fair historical catch-up, bounded XLSX parsing, report scope validation,
cookie-free conditional downloads, schedule CRUD/execution, DVIC-only role grants,
and feature independence. The native scorecard regression exercises multiple queued
performance-page requests, including a page initially selecting another station.
