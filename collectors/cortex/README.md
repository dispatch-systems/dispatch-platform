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
names another station, company or DSP. A day's meals are first asked of their keeper as the live
route data the DSP holds of today has them (`live_meals`): it reads the responses kept there
with the meal rules (`meals::from_responses`), and Cortex is asked for nothing; a job's metrics
then name `meals_live_route_data`. Without such a read the day's meals are read with no page, as
its routes are: the day's list and the itineraries of the routes with a meal Timecard does not already hold
finished, signed here and downloaded at the account's gate, each read by the meal rules
(`meals/collect/rules.rs`, which answer as `scripts/meal_rules.js` does) and let go, only its meal
evidence kept; a job's metrics then name `meals_native` as what read the day. A day that can't be
read so, as with no key on the list's page, names why (`fallback`) and is read with a page: there
meals are asked of the application's own API action, as
its pages ask, or read as moves inside an application whose action declines, each response
withheld from the page so nothing renders. A day's routes are read first with no page, with the
browser's session: every read signed here with the key Cortex gives the list's own page, and
downloaded gzipped four at a time; a key Cortex refuses, or a session it keeps sending to sign
in, has the page asked for its key once more, and the browser closes once the day is read whole.
A throttled native read (HTTP 429) pauses new requests while in-flight reads drain, keeps
successful responses for that attempt, and retries only what is missing after Retry-After
(seconds or an HTTP date), or ten seconds without one. A supplied delay is bounded at a day.
Every request waits its place at the account's gate (`connection/gate.rs`), one for each company
at each origin, which every reader of the account on this server shares: a live read and a job
that start together ask for four at once between them, and a throttle either meets cools both.
Each throttled round narrows what the gate admits next, from four at a time to two, then one,
also when the throttled read ended on a long cooldown or was cancelled; three reads in a row
that meet no throttle widen it a step. A
read asks for at most twelve rounds and waits on throttling for at most two minutes; a longer
cooldown fails promptly and stays with the gate for a later scheduled attempt. The cooldown stays with the gate
across cancellation; throttling never asks the browser to
sign or discover again, and its fixed endpoint and status are logged without identifiers.
Healthy reads download four at a time without a cooldown. `Native::counts` counts
what a read met: the requests throttled, and the narrower width since; each refusal's log event
carries the width it was asked at (`downloads`).
A reader asked for a snapshot of the day it read last asks only for routes something may be
happening on: one the list shows finished, its driver's session ended, or not begun, and says
the same of as at its last read, clock aside, keeps the itinerary read then, and one read in six
asks for them all; its counts say how many it reused (`reused`) and how many a read of them all
found changed though the list said the same (`unseen`).
`routes::Native` reads a day so for anything holding a session, outside a job too, and says why
it could not: no key on the page, the key refused, the session ended, or Cortex unavailable.
Otherwise the day is signed by the application's own API action, stopped before it leaves the
browser and downloaded four at a time with its session, and whatever does not download is read
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
tab, and no more than the account's gate admits, asks a route Cortex throttled again once the
account has cooled, never from a loaded page, reads the day's list from the response the application fetched, and reads no itinerary for
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
