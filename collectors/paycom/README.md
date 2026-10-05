# Paycom

Employees and timecards, read for one DSP through the sandboxed browser and plain HTTP. On a
first attempt with a roster of 20 or more, a few employees are read both ways and compared: if
they agree, the rest are read over HTTP, else the whole job goes back to the browser.
`collections/timecards/extract.rs` and `scripts/timecard.js` read the same pages and change
together; prove them against real Paycom with the `http_extraction_parity` probe.
