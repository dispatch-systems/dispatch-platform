# Cortex

Amazon Logistics, read for one DSP through the sandboxed browser and plain HTTP: meal breaks,
routes, posted weekly scorecards, Daily Quality and Day Safety, and DVIC workbooks. Discovery finds the station and service area
once; later collections reuse what the station's last publication stored. Meals and routes are
read as moves inside the application, each response withheld from the page so nothing renders,
and reports download cookie-free from the report host only. A meal collection waits for twelve
routes at once in one tab, reads the day's list from the response the application fetched, and
reads no itinerary for a route without meals or a finished route Timecard already holds as listed. The `dispatch-collectors` skill
has the routes' and the performance sources' full reads. Weekly and daily share the bounded
performance HTTP reader, while keeping their requests, datasets and job kinds separate.
