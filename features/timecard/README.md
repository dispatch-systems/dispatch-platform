# Timecard

Paycom's employees, punches and timecards and the meal breaks Cortex reports, on the Timecard
page (Daily, Meal Breaks and Employee Search) and its settings. Rust interprets punches and
assesses meals; the frontend only formats the results. Its addresses keep their old names
(`#dsp/<id>/paycom`, `/api/dsp/paycom/…`), and its jobs and schedules keep theirs from when
they were the only ones (`/api/dsp/jobs`, `/api/dsp/schedules`), though they serve only
Timecard's collections. It names drivers through Driver Match.
