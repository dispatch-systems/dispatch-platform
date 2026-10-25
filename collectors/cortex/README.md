# Cortex

Amazon Logistics, read for one DSP through the sandboxed browser and plain HTTP: meal breaks,
routes, posted weekly scorecards, Daily Quality and Day Safety, DVIC workbooks, the roster with
everyone Amazon offboarded, and the fleet's vans. A job's browser starts from the session its
profile kept, checked by asking for the console page and the account's companies over HTTP, and
opens no page until a collection needs one; a session Amazon no longer accepts, or sends to a
sign-in or a verification, or none at all, signs in on the page as the owner's own sign-ins
always do. Discovery finds the station's service area and the DSP's company there, kept beside
the profile for a week; performance and DVIC collections read for it, at the address the API
last answered, with no page, and find it again when the credentials change or what they read
names another station, company or DSP. Meals are asked of the application's own API action, as
its pages ask, or read as moves inside an application whose action declines, each response
withheld from the page so nothing renders. A day's routes are read first with no page, with the
browser's session: every read signed here with the key Cortex gives the list's own page, and
downloaded gzipped twelve at a time; a key Cortex refuses, or a session it keeps sending to sign
in, has the page asked for its key once more, and the browser closes once the day is read whole.
`routes::Native` reads a day so for anything holding a session, outside a job too, and says why
it could not: no key on the page, the key refused, the session ended, or Cortex unavailable.
Otherwise the day is signed by the application's own API action, stopped before it leaves the
browser and downloaded twelve at a time with its session, and whatever does not download is read
as moves. A job's metrics name what read the day (`method`) and, when native reading fell back,
why (`fallback`): one of those reasons, or `routes_native_invalid` for a day it read whole that
failed the collection's checks. A feature reads a day outside a job the same way through
`routes::Browser`, made from the DSP's Cortex browser as `Reading::browser` opened it: the day's
scope, then the day, with a native reader to keep and read the next with no browser: the one
that read the day natively, with its key, or after another way one carrying the browser's
session as the read left it, as signing in again or the application may renew it. A scope kept
for another request is refused, whichever of its day, station, timezone, service area or provider
differs, as is every driver's for a request that names none. In fixture mode no browser runs,
and each day it reads is the fixture's, as a job's is. Reports download
cookie-free from the report host only. Vans are the fleet's one list over HTTP, every status it accepts at once and
each continuation page after, for an account that sees the DSP's company alone, with the company
list the start's session check read. A meal collection waits for sixteen routes at once in one
tab, reads the day's list from the response the application fetched, and reads no itinerary for
a route without meals or a finished route Timecard already holds as listed. A meal start pressed
twice, minutes apart, is one meal from the first press; a route whose punches break another meal
rule is kept without meals, named for the rule, and the rest of the day is read. The
`dispatch-collectors` skill has the routes' and the performance sources' full reads. Weekly and
daily share the performance HTTP reader, every dataset of a week or a day at once, while keeping
their requests, datasets and job kinds separate; DVIC lists its weeks together and downloads
fourteen reports at a time. The roster's photos are shrunk as they download, to at most 400
pixels on the long side as a JPEG, a JPEG read straight at a fraction of its size, four at a time
away from the threads that serve requests, each a little more than a core. One collection brings
every photo the DSP lacks, sixteen at a time, and stops at the first the application refuses the
session for; a photo due a refresh is asked for by its ETag, so only a changed one comes again.
