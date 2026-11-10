# Paycom

Employees and timecards, read for one DSP through the sandboxed browser and plain HTTP. On a
first attempt with a roster of 20 or more, a few employees are read both ways and compared: if
they agree, the rest are read over HTTP, else the whole job goes back to the browser.
`collections/timecards/extract.rs` and `scripts/timecard.js` read the same pages and change
together; prove them against real Paycom with the `http_extraction_parity` probe.

A DSP may connect through Paycom's own REST API instead (`connection/rest/`), with the SID and Token
Paycom issues, in place of signing in: no browser runs, and nothing waits for a member's
verification. Paycom answers only the addresses the client allowed, at the one of its three
data centers that holds the client, and only the functions the client switched on, so
connecting finds the data center that takes the SID and Token and asks each function a
collection reads through once: Employee Directory, Employee, Punch History and Labor
Allocation Categories. With a pay period to count from, those are the connection's checks
(`rest::Checks`), which core keeps for its card. The period is the newest one collected, as
the feature that keeps timecards answers, or for a DSP never collected the Sunday its
credentials carry (`periodStart`).

A collection through it (`collections/timecards/api.rs`) brings the same data in the same
shape: the roster from the directory, the station from the labor allocation category named
Delivery Station Code, each position from the employee's record, and each card from the
employee's punch history (`punches.rs`), a shift's punches kept on the day it began. The API
offers the punches alone, not the hours Paycom's timecard totals, so a day's hours are the
time between each in and its out, plus hours entered without a punch; an answer in a shape it
can't place fails the read (`paycom_api_unreadable`) rather than becoming a wrong card. It
isn't offered in Production (`paycom_api_unavailable`) until those hours are proven against
Paycom's own totals. In fixture mode the Token chooses how connecting goes (`connection/rest/fixture.rs`):
`refused-token` is refused and `punch-history-off` finds that function off.
