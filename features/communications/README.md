# Communications

The DSP's direct messages with the members of its Slack workspaces, and the texts its dispatch
phones relay. Employees write to the DSP's Dispatch app in Slack, in its Messages tab; anyone
texts the DSP's phones; anyone at the DSP whose role gives them Communications reads each
conversation on the Communications page and answers it there. A Slack answer reaches the
employee's direct message under the full name and photo of whoever wrote it; a text answer goes
from the DSP's phone as written. Slack's part works on Slack's own foundation for direct
messages: while it's on in a workspace, Slack no longer answers what's written to the app that
DSP Messaging is unavailable. Texts come through the DSP's own dispatch phones
([Dispatch phones](#dispatch-phones), [Texts](#texts)): drivers' work phones on Today's Routes,
and every other number's in Recents.

- **Switch:** `communications`, off for every DSP until the platform owner switches it on: its
  texting with it, as Messaging ([below](#messaging)) is part of it, and its phones too. Its code depends on Slack
  (`depends_on: ["slack"]`), which a build without Slack leaves it out with; for a DSP, it needs
  neither: without Slack on, it hears nothing of Slack, and without a phone paired, nothing of
  texts. A DSP's switch record from when Phones was a feature of its own, `phones`, is left as it
  was and read by nothing.
- **Permissions:** `communications.use` (Use Communications) covers reading the DSP's
  conversations, who each is with, and answering them. Owners hold it; another role holds it once
  the DSP's owner turns it on. Every route is behind it, but the image of someone's initials,
  managing the phones, and the phones' own address. `phones.manage` (Manage Phones), apart from
  it, is for the phones' card: pairing, renaming and removing the DSP's phones, as core's
  `connections.manage` (Manage DSP Connections) allows too, which exists while Communications is
  on, as its phones are the DSP's own (`dsp_connection`). Owners hold it; another role holds it
  once the DSP's owner turns it on, as it did while Phones declared it.
- **Its integration:** `communications.messages`, under each workspace's Integrations in Slack's
  settings, on from the start (`starts_on`) in every workspace the DSP has, those it had before
  too, until someone who manages Slack switches it off there; switched off, it stays off there.
  It hears of each message written to the app (`message.im`), and needs what Slack asks for
  itself too, to read and answer direct messages and tell who members are (`im:history`,
  `chat:write`, `chat:write.customize`, `im:write`, `users:read`, `users:read.email`), declared so
  a workspace that hasn't approved them needs updating for it; and `files:read` to download what
  members attach, `files:write` to upload the photos someone at the DSP adds, and `im:read` to
  list the app's direct messages to catch up on.
- **Settings:** `communications.display` (Message display), each member's own (`.member()`),
  under `communications.use`, on Settings' Display page under Personal: drawn by its own component,
  `frontend/display/`, with a preview of the page beside the choices, and saved as they're made.
  Its values (`MessageDisplay`) are how the page looks to that member, which the page reads with
  Settings: its text size, line spacing and spacing; the messages across the conversation or in a
  centred column, bubbles or plain, your team's side, bubbles' shape and width, each message's
  time, each run's, on hover, or above the first after a pause of ten minutes, photos and your
  team's names by them, each where the team's writer changes; your team's and the employees'
  colours, the employees' white, edged, on a tinted conversation to start; the list's width, rows,
  preview lines, photos and waiting times; the profile beside or over the conversation, its width,
  its slide, and whether it opens with each conversation; and whether Enter sends. A phone (a
  touch screen whose shorter side is under 500px) has its own of those that apply on one,
  `phone_<choice>`: no layout, list or profile width, profile beside or kept open, or times on
  hover; and Enter starts a line there to start. Then, on any screen, how a new message alerts
  them, under New messages: `sound`, on to start, and `notifications`, off, which turning on asks
  the browser for, staying off if it refuses, and which shows on only in a browser that has
  allowed it; offered where the browser shows one from the page, or can be pushed to while the
  server pushes (as a phone's can: [New messages](#new-messages)), and left out elsewhere.
  And `communications.quick_replies` (Quick replies), the DSP's, under `communications.use`, so
  anyone who uses Communications edits them, on Communications' own page under Features: drawn by
  its own component, `frontend/replies/`, as a list of texts is none of Settings' kinds of value.
  Up to 30 short replies, each up to 500 characters, in their order, each in a box of its own with
  Remove, Add reply under them, and Save, which leaves out the empty ones; before the page moves
  on with them unsaved it asks "Save changes?", Save or Discard, and closing the tab drops them
  silently; a save over another session's since is refused
  (`settings_changed_reload_before_saving`) and the list read again.
- **On every page:** `communications.alerts`, a watcher (`frontend/alerts/`), for those with
  `communications.use`, alerting them to new messages ([New messages](#new-messages)), and to the
  DSP's phones as each goes down; and `communications.phones`, a watcher drawing a banner atop
  every page while one is down ([Dispatch phones](#dispatch-phones)).

## The page

- **Communications** in the sidebar, for those with `communications.use`, with how many
  conversations wait beside it, Slack's and the numbers' texts (`waiting`, as the banner of phones down reads it, every 30 seconds
  on every page, and as the number changes while it's open). The page fills the window, however wide (`fills`),
  the list beside the conversation open, each scrolling on its own, and everything on it grows
  with the page's own width, smoothly, from a laptop's to an ultrawide's, then stops (a
  container query on the page). On a wide page, the messages and the box under them keep to a
  centred column. Under 760 pixels of page, as on a phone, one pane at a time, edge to edge:
  the list, or the conversation open, which takes the whole page, with a way back; one opened
  from the list is a step of the browser's history, so Back, the phone's too, goes back to the
  list. The conversation open is in the address (`?open=<id>`), and a link to one opens it, the
  page open or not.
- **Sections:** the list's, at its top while there are two or more, each with how many wait in
  it: **Recents**, always, every other section's conversations and groups and the numbers that
  text the DSP's phones ([Messaging](#messaging)), counting everyone waiting; **Today's Routes**
  (Routes on a phone or beside two others), the drivers on a route today and their work phones,
  while the DSP has live route data ([Today's Routes](#todays-routes)); and **Slack**, its
  workspaces' direct messages and the groups it messaged, while it has a workspace. The one shown
  is remembered in the browser for each member at each DSP; until one is chosen, the page opens
  on the first that lists a conversation as it first reads, else the first; a conversation or
  group opened, as from a link, shows its section, unless the one shown lists it too, as Recents
  does; choosing a section leaves open what it lists.
- **Phones offline:** in the heading, a chip for each of the DSP's dispatch phones that isn't
  online, with how long since it was last seen, by its name when the DSP has several (Phone
  offline · 14m), as the overview and each update bring them.
- **The list:** in Slack, with two or more workspaces where Communications works, which it
  shows, every one or one of them, each by its logo (else the first letters of its name), as the Update
  popup, Copy chat link and the profile's Slack row show it too, with how many of its
  conversations wait; that choice is remembered in the browser for each member at each DSP, and
  reads as every one once the workspace is no longer one of two or more where it works. Then
  Search, and Messages or Contacts, whichever Search searches, by a name, misspelt or not,
  marking the letters that matched.
- **Messages:** Waiting, the longest waiting first, each with how long it has in plain words, "31
  min", "2 hr", "3 days" (read aloud in full, "Waiting 31 minutes"), amber from 15 minutes and red
  from 30, and beside it a check, Done named for whom it's with, that marks it done without a
  reply; then Responded, the latest
  first; older ones read as the list is scrolled to its end, and once a read of them failed, as
  Try again is pressed. Each shows who it's with, by their
  photo, when they last wrote, and what was written last, under the first name of whoever at the
  DSP wrote it, or who at the DSP is writing in it. In Recents, every section's, Waiting, then
  those answered under the day each was last written in (Today, Yesterday, then the day), the
  latest written in first under each, each face marked with where it is (Today's Routes, by text, or Slack), a driver's with their
  route as Today's Routes shows it; down to where the first of the sections it pages stops, the
  next of them read as it's scrolled to its end; and Search finds them in every section.
- **Contacts:** everyone the DSP can message, A to Z under each first letter: their photo, name
  and role, and with every workspace shown, the one they're in; how long they've waited, or
  when they last wrote. Each opens their conversation, or starts one, as New message does.
- **A conversation:** who it's with, the workspace it's in while there are several, and, while
  it waits, how long, "Waiting 31 min", and Done, by its word on a phone too; once it's answered,
  by Done or a send, **Next waiting**, while another of the section shown waits, which opens the
  one waiting longest, a group's too, never by itself; its messages under the day each was written, the member's beside
  their photo, the DSP's under the name and photo of whoever sent them, each edited in Slack
  marked Edited, a reply in one of its threads in its place among them, photos shown and other
  files to download, or why one isn't kept; older ones read as it's scrolled to its start, and
  once a read of them failed, as Try again is pressed. A read that failed says why, with Try
  again. Right above its box, however its messages are scrolled, who else at the DSP is writing
  in it, "Keisha is typing…", said to a screen reader as it changes. Writing in it: text, with
  photos added (**Attach Image**, under the plus at the box's start), or a GIF in their place
  (**Choose GIF**, there too, [GIFs](#gifs)), as much as one message carries as the conversation's read says
  (`limits`), which what's refused says too, and nothing until it's read; **Quick replies**,
  beside the smiley while the DSP has some, on a phone under the field with attaching and Send,
  opens them, and the one chosen goes in where the writer was, as an emoji does, never sent by
  itself, in a group's box too; Enter or **Send** to send; while writing, teammates see it. Where a teammate, or the phone itself, wrote in it since
  the member started writing, since it opened with what they'd written, or since they last
  answered this, Send asks first, above the box: "Keisha just replied", with **Send anyway** and
  **Cancel**, which keeps what's written and shows the reply. It's told from what the page holds,
  the latest read's messages and the last each update brings, never older ones read since, by
  which were held, not when each was written; a reply once held counts until the member answers
  it or starts a new draft, as an empty box gains text or a photo, which also puts the question
  away (`frontend/conversations/replied.ts`).
  What's written and not sent is kept, each conversation's and each group's, while the
  page is open and in the browser's tab for each member at each DSP, so it's there again as it
  opens again, after a reload too (`frontend/conversations/drafts.ts`); the photos go as it
  closes. Sent, what was written and photos added while it went stay, and the conversation shows
  as the server answered, unless an update brought it written in since. Photos
  show by Driver Match's person for each (`/api/dsp/people/{code}/photo`), never Slack's own
  addresses. Each conversation read is kept while the page is open, for forty at most, so it
  opens again at once as it was read, and is read again as it opens only once an update changed
  it, or one of whom it's with's others it shows, since; the one open, at once as one does. A row
  pointed at or reached by the keyboard, and the first three waiting of those shown, are read
  before they're opened, two at a time at low priority, but not on a slow or metered connection
  (`frontend/conversations/reads.ts`).
- **The profile** of whom a conversation is with opens from who it's with, above it, or their
  photo by their messages, over the conversation, and closes with its X, Escape, or a press
  anywhere else; from 1,500 pixels of page, it opens beside the conversation instead, a column
  of its own covering nothing, and closes with its X or Escape. Its X and Escape put focus back
  on what opened it. Their photo, name and role; Call, while they have a phone number; Today:
  their route on the DSP's own day, its van, when its wave starts, how many stops it has and are
  done, and its pace; and while live route data holds today, the work phone they have, in full;
  Contact: their phone numbers, their default first and marked Default while they have others,
  which Call calls, email address and the workspace the conversation is in. Each is
  left out when nothing knows it, and Today when they have no route today. No feature has a page
  of a person to open, so it has no Full profile.
- **New message** finds whom to write to by name, misspelt or not, and opens their
  conversation, or opens a direct message with them, kept at the top of the list until it holds
  a message. With nothing typed, Message a group comes first.
- **Keys,** on a desktop, but while a field is written in, or with Ctrl, Alt or ⌘ held: J and K
  open the next and the previous row of the list, or its first with none open; N presses Next
  waiting and E Done, in the head of what's open, each naming its key (`aria-keyshortcuts`, and
  its title); / goes to Search; and Escape leaves the field written in. Keys pressed in a dialog
  are its own (`frontend/shared/shortcuts.ts`, `frontend/page/useShortcuts.ts`).
- **Display** opens the member's own Message display in Settings, which the page looks as, with
  either section.
- **Copy chat link** copies the address that opens the app's Messages tab in a workspace where
  Communications works, chosen from a menu with several, for employees to write to the DSP.
  It and New message show only with such a workspace.
- **Setting Slack up**, for those who also hold `connections.manage`, which connects and updates
  the DSP's workspaces: while the DSP has none, and no phone paired, Connect Slack takes the
  inbox's place; while a workspace where Communications is on hasn't approved all it uses
  (`needsUpdate`), Update Slack covers the page while it shows Slack, not the sidebar, with no
  way to close it but to Recents while the DSP texts; each such workspace with Update and Copy
  link (its own install link, for whoever approves apps in that Slack), until an update brings
  the workspaces with none needing it, within twenty seconds. Connecting or updating from here
  comes back here from Slack, which finishes it. Everyone else sees the page as it is, empty
  without Slack.

## Today's Routes

The drivers on a route today, while live route data holds today, and the work phones they text
from. At the stop, as live route data empties, it lists nobody (No routes), and the conversations
stay, the work ones waiting two hours more ([Waiting](#texts)), each driver's answered at the work
phone they had at the stop, which their conversation shows, with the day's route, until then; the
next day's drivers fill it as its live reads start.

- **Who:** each driver its routes name today, with their route as last read live (of several
  itineraries, the one with the most stops), and their work conversation once they have one;
  each driver gone from the day's list, once their conversation was written in today; and each
  work phone nobody had that texted today, as itself (Work phone 5521).
- **The list:** the drivers by name, A to Z, waiting or not; then the work phones nobody had (No
  driver), the latest written in first. Each row as a conversation's in the other sections, and
  under it the driver's route code, the work phone they have now by its last four digits, a bar
  of the stops done, `98/168` (read aloud "98 of 168 stops", as the board and the profile show
  it), in the colour of its pace, and its pace but on time: Behind, At risk (Amazon's `BEHIND`,
  `AT_RISK`), or Done, every stop of it. A driver without a conversation
  yet opens one, to text them first.
- **The board:** beside Search, List or Board, each by its word, remembered in the browser for each member
  at each DSP, and not offered one pane at a time: the same groups as a table across the page,
  each driver a line of route, wave, van, work phone, stops and pace, and the last text with how
  long it has waited or when. It shows while nothing is open: one chosen there, or opened by a
  link or a chip, opens in the list, and the board comes back once nothing is; choosing the board
  leaves what's open.
- **Search** finds work conversations, across days, by a driver's name or the digits of any work
  phone they texted on.
- **Fresh:** read when it or Recents shows, again every minute while the page is in view
  (`ROUTES_READ_MS`), as live reads say nothing as they come, at once when an update changes a
  work conversation, and as the page comes back into view or the browser back online, a read that
  failed tried again, as the dashboard's reads are. Until a read answers, loading, or offline or
  paused, as the dashboard's pages say it, never No routes; a read that fails says why in Today's
  Routes, the list or the board, and nowhere else. A driver's work conversation open shows only
  with one of the two (opening it shows Today's Routes, unless Recents shows; choosing another
  section closes it), so its route, van, stops and pace follow this read, as an update changes
  it only as their work phone does.
- **A driver's conversation:** under its head, their route code, van and the work phone they have
  now, in full, as each of their conversations' heads says it ([Elsewhere](#elsewhere)), and how
  long it has waited; chips to their conversation of each kind, Work (by the work
  phone they have), Personal (by their own number) and Slack, each marked with a dot while it
  waits, opening it or starting it ([Elsewhere](#elsewhere)). Each workday begins with a line
  across it, Today · Sun, Oct 25, Yesterday, or the date, with that day's route and van and the
  work phone its first text went through; earlier days dimmed; and where they texted from
  another work phone that day, Switched to 7643. With no work phone now (none shown, theirs
  another driver's latest too, or once the day stopped), No work phone takes the box's place, with
  chips to their other conversations; what was being written in it stays, its pictures too, and
  comes back with a work phone.
- **A work phone nobody has:** No driver under its name, and **Pick driver**: one of today's
  drivers, found by name or route, each with their route and wave; picked, the phone's texts of
  today are theirs while the day's route data names no one driver with it ([Texts](#texts)), and
  their conversation opens. Escape closes it, focus going
  back to Pick driver.
- **The profile** of a driver, from their name or photo: the ways to reach them, as the chips;
  Today: route, van, wave, stops with their bar, pace, and the work phone, as the
  conversation's head shows them, one source for both: whichever answered last of Today's Routes'
  read of the driver, while it shows, and the conversation's own, so a phone withdrawn shows at
  once and another driver's change takes nobody's progress back; none once Today's Routes says the
  day is over; Contact: their phone numbers, their default marked, email address and Slack
  workspace.

## Elsewhere

A person's conversation of each kind, which the page goes between: their work conversation, as a
driver who had the DSP's work phones, while they work today, with a route today or a work phone
now; the one by text with their own number, their default, the
first of Driver Match's numbers for them, one added to theirs before their sources' (a driver's
new phone, so not the one they last wrote from); and their Slack one, in a workspace where
Communications works, the latest written in, else their first account there. Shown, as chips,
on each of them while one is another than the conversation open, each waiting as the page's list has it, so its dot
follows each update, and one started since is found by whom it's with. A driver with a work phone
now has the work conversation of the ID that has it, as Driver Match may know them by several,
started once they have none there; of several IDs with one, however many, the one whose route
today has the most stops, the route their Today shows, else the first linked; else the latest of
any of their IDs. The head of each says alike, as pieces side by side on one line, short of room the van and a
workspace giving way first:
their route code and van today (CX14, Van 214), as their profile reads their route, none while
they drive none, then how this conversation reaches them: the work phone they have, the number
it's with, while it isn't their name, or in Slack their role, and the workspace while the DSP
has several.

## Messaging

The numbers that text the DSP's phones, in Recents: drivers' own phones and any other number,
from people who may or may not work for the DSP.

- **Who:** each number's conversation is with the person Driver Match has that phone number for,
  by its last ten digits (of several, one who hasn't left first, then by name), by their name and
  photo; else by the name the DSP saved for it; else the number, `(425) 555-0199`, by a phone.
- **Messages:** in Recents, among every section's, with Search finding them by name, misspelt or
  not, or by a number's digits; a blocked number's isn't listed.
- **Contacts:** Recents', while the DSP texts: everyone Driver Match has a phone number for who hasn't left, by each number they have, and
  every number the DSP saved a name for, each number once, A to Z, each with its number and how
  long it has waited or when it was last written in, opening its conversation or starting one;
  found by a name or digits typed. Then **Blocked**, each number blocked with Unblock.
- **A conversation:** who it's with, their number while it isn't their name, and while it waits
  how long and Done; **Save contact** (Edit contact once saved), while Driver Match knows no one
  by the number: **Add to driver**, the number added to someone Driver Match knows who hasn't
  left, today's drivers first, found by name, so its texts are theirs everywhere people are shown
  and it's their default number, the profile's Phone and Call and the Personal chip's
  (`people::add_phone`: Driver Match keeps it, ahead of their sources' numbers, which stay theirs;
  one person may have several, the latest added their default, and each number keeps its own
  conversation); or **New contact**, a name, up to 80
  characters, shared by everyone at the DSP. A driver picked or a name changed is never left
  unsaved without Save changes? asking to save or discard it, and Escape closes it with focus
  back on its button. Its menu: **Remove from** whoever the number was added to, **Copy
  number** and **Block**, once asked "Block (425) 555-0199?", or **Unblock**. A number blocked leaves Messages and the counts, and
  what it sends while blocked never makes it wait, then or once unblocked; its texts are still
  kept. Its texts as Slack's messages show, the other party's by their face or a phone; the DSP's
  under who wrote each, or the phone's name and mark for one sent from the phone's own messaging
  app; each picture shown, as the phone keeps it (PNG, JPEG, GIF, WebP or AVIF; an iPhone's HEIF
  photo as the JPEG the phone's app makes of it); each video as the picture of its first frame
  under a play button, played over the page once pressed, or downloaded where the browser can't
  play it, up to 25 MB; any other file a download. A video the phone keeps (below) is asked of
  the phone as it's pressed, and plays once the phone sent it: Play says so while the phone is
  offline or has it no more. Under a text the DSP sent, the last of a run or
  one that failed: Sending, Sent, Delivered, or Not sent with why, where it's known, and
  **Retry**, but in a conversation it moved to: Phone offline (`phones_offline`, or the phone's
  `expired`), Phone removed (`phones_removed`), or the phone's own words, as "No service".
- **Writing:** text, with pictures (PNG, JPEG or GIF, by their stated type), as much as one text
  carries (`compose::TEXT`, as the phones hold one to: 10,000 characters and ten pictures of up
  to 20 MB each), Enter to send, the number's last four digits beside it while the
  name doesn't say it; teammates see who's writing, as in Slack's. As in a driver's work
  conversation.

## Dispatch phones

Android phones running Dispatch's app, each paired to one DSP, relaying the texts and pictures
its number receives and sends, and sending Dispatch's texts from that number. No service stands
between: a phone opens its own connection to its DSP's address. The phone side
(`backend/phone/`) knows the phones, the texts they relay, each attempt at Dispatch's and how it
went, and their bytes; never who a text is from or which conversation it's in, which filing
decides ([Texts](#texts)).

- **The card:** on Settings' DSP Connections, for those who manage the DSP's phones or all its
  connections (`connectionPieces`, `dsp`), while Communications is on: each phone, by its number,
  with its name, model and the app's version, tagged Update while that's older than the one the
  DSP's address hands out, Online once it checked in in the last two minutes, else Offline and
  for how long, read again every 30 seconds; what needs attention on it, as it last said it stands
  (below), in plain words beside it: Can't send texts, Can't read texts, No SIM, No signal, Setup
  not finished, how many texts wait to go once ten or more do (12 texts waiting to go), and how
  many pictures it received that its texting app hasn't downloaded; Rename and Remove in its menu; a name changed and
  not saved asks "Save changes?", with only Save and Discard, before the dialog closes; closing
  the browser tab drops it silently. Its routes are refused (`not_found`) while
  Communications is off, though managing all the DSP's connections still lets a member in. With
  none, No phone. **Add phone** shows the code that pairs the next phone, as a QR code its app
  reads and as its eight letters, and how long it lasts; one expired, or replaced by a newer one
  as another session makes, gives way to a new one with New code; the dialog closes once its own
  code paired a phone, asked after every three seconds until it paired, was replaced or expired,
  and once more as it runs out. A code needs the DSP's own address, for the phone to reach:
  without one, `phones_address_required`. While the DSP's address hands out the app
  (below), **Download app** beside Add phone downloads it, and Add phone puts Install first, its
  download as a QR code with its name and version (DSP Dispatch: Relay Dev 1.4.0), then Pair.
- **Pairing:** a code is eight letters of 32 (no 0, O, 1 or I), made by someone who manages the
  DSP's phones, working once, for ten minutes, one at a time: a new one replaces any not used
  yet. Only its SHA-256 is kept, with an ID of its own, by which the card asks how it stands
  (`GET …/phones/pairing/{id}`): waiting, paired, replaced or expired. The phone reads it with the
  DSP's address from the QR code (`dispatch-phone:pair?site=<address>&code=<code>`) and trades
  it, with its own number, model and the app's version, for a key: 32 random bytes in
  hexadecimal, of which only the SHA-256 is kept. The first phone is named Dispatch, the next
  Phone 2 and so on, until renamed. A code tried is told right or wrong by a read, so a wrong one
  never takes the write lock, and counts against the address it came from and the DSP, in the
  server's memory for ten minutes: past ten from an address, or a hundred at a DSP from every
  address, each is refused `rate_limited`, a right code too, until they're older.
- **How it stands:** each phone says it with each wait on its outbox ([its
  protocol](api/phone/README.md#how-the-phone-stands-with-each-wait)): its Set up steps undone,
  whether it may read and send texts, its default texting app, its SIM, what waits on it to go, picture messages not
  downloaded, why a text last failed and the last error it didn't expect. Dispatch keeps the
  latest of each phone, written only as it changes.
- **Down:** a phone an app holds, not the demo's stand-in, that has been offline ten minutes or
  more, or says it can't send texts (Android doesn't let it, or it has no SIM): a banner atop every
  page says so to those with `communications.use` (Dispatch phone offline for 25 min, or by its
  name while the DSP has several; Dispatch phone can't send texts), linking to the card for those
  who may open it, gone as soon as it's back. It reads them with `waiting`, every 30 seconds on
  every page, whatever the member chose for alerts, which the sidebar's count shows too, and again
  as the page's updates see them change. The tab
  alerting the member of new messages notifies them too, once as each goes down, as they chose
  Notifications in Message display; one down already as the tab first looks notifies no one.
- **Removed:** a phone removed keeps its texts; its key works no more, and its next call is
  refused as `phones_removed`. The texts Dispatch queued that still wait on it fail as it's
  removed, with `phones_removed` as their error, filed as they fail; those waiting on a phone
  offline fail as they expire ([Texts](#texts)). A phone calling an address no DSP has, as once its
  DSP is deleted, is refused as `phones_removed` too, and unpairs.
- **Their address:** `/api/communications/phone/…`, at the DSP's own address alone, never from a
  browser (a request with a cookie is refused) but for the app, while the DSP is active and
  Communications is on there (`phones_off` otherwise): pairing by the code, and the app, public;
  every other call by `Authorization: Bearer <key>`, which the route checks itself, as Slack's
  signed events are checked (public to the server, which no session reaches). A phone waits on its
  outbox for what Dispatch has for it (woken by its own live channel, `phones`, apart from the
  page's), reports the texts it receives and sends from its own messaging app, each kept once by
  its own ID of it, sends their pictures' bytes, fetches those of Dispatch's, and says how each
  attempt at Dispatch's went. A video stays on the phone: only the picture of its first frame
  comes with its text, and its bytes once someone plays it, as the outbox asks for them, kept an
  hour (`FETCHED_MS`) so another who plays it soon after has them at once, then let go of. Waiting
  on the outbox and reporting texts or statuses count as checking in, noted at most every thirty
  seconds (`SEEN_MS`), as bookkeeping no cached read depends on; a file's bytes, either way, don't.
  Each call, with every payload, unit and limit, is in the [protocol reference](api/phone/README.md)
  beside the code.
- **The app:** DSP Dispatch: Relay, which the DSP's address hands out (`phone::app`), as the
  build packed it: CI's signed builds, named in `relay.json` beside their APKs in the runtime's
  `dashboard/relay/` ([Android apps](../../android/README.md#releases)). A preview server, as
  Dev is, hands out the Dev build, which `main`'s and a release's packages carry, and Production
  its own, which only a release's package carries; none while the build packed none.
  Anyone at the DSP's address, a browser too, reads its latest version's details (version,
  `versionCode`, size and SHA-256) and downloads its APK, as a phone installs it before it's
  paired: the app anyone may download from its GitHub release, and nothing of the DSP's. Each
  outbox answer carries its latest version, so the app updates itself within a wait, and each
  wait says the app's own, which Dispatch keeps as the phone's once it changes.
- **Limits** (`phone::LIMITS`): what one text may carry, a body of 10,000 characters and ten
  pictures of 20 MB each at most, which answering is held to (`compose::TEXT`), and the phones'
  reports, before any of it is kept.
- **Numbers** are kept one way (`phone::numbers::normal`): `+1` and ten digits in North America,
  `+` and the digits with a country code, a short code's digits alone, so a number typed or a
  person's matches texts'. **IDs**, of phones, texts and files, are opaque, at most 64 letters,
  digits, `_` or `-` (`phone::model::identifier`).
- **Demo:** a development or preview DSP has a stand-in phone, `(206) 555-0100`, which no app
  holds, with three texts it received; a text Dispatch sends from it is delivered at once.
- **Log:** `phones.paired`, `phones.renamed` and `phones.removed`, under Connections.

## Texts

What the DSP's dispatch phones relay, as the phone side keeps them, one row each: filed in its
conversation as it's kept or changes; and answering, or sending again one that failed, which
queues a text for a phone.

- **Filed:** in the same write and transaction as the phone side keeps a phone's report, a
  status, a picture's bytes, a phone removed or an answer queued (`filing::changing`), each text
  new is placed in a conversation (`communications_text_conversations`), on its own row
  (`phones_texts`, by the phone side's ID, with its phone, direction, number, body, when it was
  written, who at the DSP sent it, how it went, and its pictures as the phone side keeps them),
  and each conversation a change touched keeps whether it waits; the pages waiting on it hear
  once both are committed. A text that can't be placed as it comes, as reading where it belongs
  failed, is kept all the same, waiting to be filed (no conversation yet, and since when it
  couldn't be placed, logged as `communications_texts_failed` with its DSP and text), and every
  minute's upkeep places those waiting, oldest first, each on its own, writing only while one
  waits; one still waiting ten minutes after it first couldn't be is placed under its number, as
  on a day Routes can't read, and its day noted to be filed again (below), which moves it where it
  belongs once Routes can say. Where it's placed:
  - **A work phone's**, a driver's: a number `+1` and ten digits, as Routes' work phones are, is
    filed by the day it was written, where the DSP is, or, in the two hours after a stop that run
    past midnight, by the day stopped, as its drivers are answered then; under the driver whose
    latest work phone it is that day, by their ID of the source naming the drivers, as Routes reads
    the day (`day_phones`): live route data's while it holds that day (`live_route_data`), else
    historical route data's (`driver_routes`). The whole day's texts with a number are that
    driver's, both ways, so a phone handed to another driver takes the day's conversation with it;
    who had it earlier that day isn't told apart. A driver's conversation is theirs across days,
    whichever phone they had. Where the day names no one driver with the number, nobody's latest
    phone that day or two drivers', the driver someone at the DSP picked for it that day (Pick
    driver, `communications_picked`); else the work phone's own, with no driver, a number nobody had
    that day but ever had as one (`ever_work_phone`) too. Both are listed in Today's Routes.
  - **Any other number's**, Messaging's, by the number.
  - **A day Routes can't read**, as one not collected, gone, emptied unsaved or not read since
    Routes kept its phones, or while Routes is off for the DSP: a text kept already stays where it
    was filed, and one that comes is placed under its number, the work phone's own while Routes
    says it ever was one, until the day can be read. A text is placed as it first comes; how it
    went, or its pictures coming, never moves it.
- **Filed again:** a day's route data changes as the day goes, and may reach historical route
  data after its texts, as a text a phone relayed late; so each time Routes says a day's route
  data changed (`routes.data_changed`, which names the day), every text of that day, both ways,
  is filed again where it belongs now: a text that came before the day's first live read goes to
  the driver whose phone a read then names. A day Routes can't read keeps its texts where they
  are. Each day heard of is noted first (`communications_refiling`), and let go once its texts
  are filed again, but while Routes is off for the DSP, which says nothing of it; so one whose
  filing again fails, logged as `communications_texts_refile_failed` with its DSP and day, or
  that a restart cuts short, is filed again by every minute's upkeep, the earliest four noted
  first, each on its own, while Routes is on, until each is done. Today's are filed again every
  minute too, so a change nothing heard, as while Communications was off, is repaired by the next
  pass, which writes only once one belongs elsewhere, a failure logged with its day; an earlier
  day's only as Routes says it changed, or as it's noted.
- **Waiting:** as a Slack conversation's: a text in after the latest text out that didn't fail,
  and not covered by a Done. Done marks each text in it covers, those up to the latest the page
  showed, which stays handled wherever it's filed again, never handling a text it didn't cover. A
  failed answer leaves it waiting; one queued counts as an answer until it fails. A work
  conversation waits only on what was written on the live day, while live route data holds
  today, and for two hours after it ends, at the stop: from the first text in on it after its
  latest answer or Done, though it came before the day's first live read. A text in during those
  two hours waits, and counts, as on the live day, until answered, Done, or the two hours pass;
  then none waits or counts, and the next day starts afresh. When the live day ended is kept as
  Communications first saw it (`communications_live_days`), as it hears today's route data change
  or by the next minute's upkeep, never after the day itself ended, as for a day a stop never
  saved. Each time today's route data changes, the work conversations waiting by their texts are
  noted changed as the live day began or ended, so the pages waiting on changes hear it, and each
  driver's whose work phone changed since the server last heard (`LiveDay::phones`), so one open
  hears it, or none shown any more: kept in its memory, so every driver's on the first change
  since it started. As the two hours after the stop pass, so are the work conversations waiting,
  and every driver's, as any may have had a work phone at the stop, so the pages hear them stop
  waiting, and that phone go: from Communications' own conversations, so Routes failing to read
  the day then never holds it up.
- **Answering:** what someone at the DSP writes goes as written, nothing added, from the phone that
  last texted that number, else the one seen last, one online before any offline, with its
  pictures, kept by the phone side as they're added, the adder's alone until they go, and none of
  one kept should the adder's access end while its bytes come. A number's conversation, and a work
  phone's nobody had, answer their number; a driver's, the work phone they have now: one they were
  picked for today (Pick driver) that's no one driver's latest, as its texts are filed under them;
  else their latest of today, as live route data reads it, but one that's another driver's latest
  too, which is no one driver's; for the two hours after the stop, the work phone they had then,
  as the day saved reads it, alike; refused while they have none, as once those two hours pass
  (`communications_no_work_phone`). Today's Routes and their Work chip show the same phone, and
  their conversation, for those two hours, the one they had at the stop. Filed
  as it's queued, under the full name of whoever sent it (`sent_by`), taken as it's first filed,
  however it was sent, keeping the conversation it was written in as it was queued (`queued_in`),
  wherever it's filed. Retry sends again one Dispatch sent that failed, from the phone a new text
  to its number would go from (`phones_none` while none is paired), only while it's filed in the
  conversation it was written in, refused once it's in another (`communications_text_moved`), as
  with a work phone handed to another driver, which takes the day's texts with it, or one filed
  as it went under a driver other than the work phone it was written to; and only where a text
  written now would go: in a driver's, refused while they have no work phone
  (`communications_no_work_phone`) or have another (`communications_work_phone_changed`), as the
  one it went to may be another driver's since. Either is queued, and filed, in the write that
  checks where it goes and the member's access (`phone::outbox`), so a handoff, the stop or a
  change of their access can't come between.
- **Expiry:** a text queued, or tried again, goes only within ten minutes (`EXPIRY_MS`): the phone
  never starts it after, and says it failed (`expired`). One still waiting as it expires on a
  phone offline then, not checked in for two minutes, fails, by every minute's upkeep, with
  `phones_offline` as its error, filed as it fails, so its conversation waits again and Retry
  sends it anew. Should the phone say that attempt was sent after all, it counts.
- **Log:** in the DSP's audit log, under Settings: a name saved for a number, or changed,
  `communications.contact_saved`, by the number, with the name before and after; a number
  blocked or unblocked, `communications.number_blocked` and `communications.number_unblocked`,
  by who it's with and the number. Saving the name it has, or blocking a number blocked
  already, changes nothing and logs nothing.

## Messaging a group

One message to many members at once, as "the 9:20 wave is running 20 minutes late": each gets it
in their own direct message with the app, and their answers come back together.

- **Whom to:** Message a group, in New message, opens it beside the list, where the message being
  written sits at the top. To finds people as New message does, and groups by name; each pick is a
  chip, taken out again with its X, or Backspace. Below it, the groups to choose from, each ticked
  and a chip once picked, and each left out while it holds nobody: **Today**, everyone on a route
  on the DSP's own day, then each wave by when it starts, as Routes has read the day
  (`driver_routes`), while it's on and has read some; **Roles**, drivers, helpers and the office,
  as Driver Match knows them; and with two or more workspaces where Communications works,
  **Workspaces**, everyone and each workspace's. Today's and the roles' groups hold their people
  in every workspace where Communications works; a workspace picked narrows them to it, and picked
  alone is everyone in it; someone picked by name always gets it. Each person gets it once, by one
  of their accounts: the one picked by name, else the first in a workspace picked, the one last
  written in first. It's called what was picked: one pick's name, else theirs joined, someone by
  their first name, and past three the first two and how many others.
- **Writing it:** as in a conversation, photos too, sent with Send to and how many it goes to.
  Whom it's to and what's written are kept while another is open, and Message a group comes back
  to them, until it's sent.
- **Sending:** in the background, one at a time, at most one a second in each workspace, as Slack
  asks of an app posting messages (`chat:write`), in each person's direct message, opened first if
  need be (`im:write`), under the sender's full name and photo as an answer goes
  (`chat:write.customize`), its photos uploaded once to each workspace (`files:write`). Slack
  asking Dispatch to slow down (429, Retry-After) is waited out as long as it says, up to two
  minutes, and beyond that tried again two minutes later; one to a workspace where Slack needs
  updating or reconnecting waits, uncounted, five minutes at a time; a try that fails is tried
  again after a minute, five and fifteen, then given up; one to a workspace where Communications
  no longer works, or someone no longer in it, isn't sent at all. None goes out more than two
  hours after it was written, as while Communications was off: a wave running late has moved on,
  so it's given up as too late, or as what it waited on, when that was Slack: no post starts
  past then, a photo's post tried again or a token renewed first included, and a post doesn't
  wait on Slack asking it to slow down. Each is kept until it's sent or
  given up, so sending picks up after a restart, from the pass every minute; a try that may have
  posted before the server stopped looks for its post in their direct message first
  (`im:history`, every page since), by the metadata each post carries, and keeps it rather than
  send a second, even past the two hours. What it holds in memory
  of a DSP's sending goes as the DSP is deleted (`deleting.forgotten`).
- **The group** is listed among the conversations: two of its people's faces, its name and how
  many it went to, and the latest answer under its writer's first name, or the DSP's message. It
  waits while any conversation listed under it waits, by the longest wait, with a check that
  marks them all done, and it's counted as people: the counts go by those waiting. With one
  workspace shown, it's listed while any of its people are in it.
- **Under the group:** a message to a group answers no one. Someone's conversation is listed under
  the group the DSP last wrote to them in, while all they wait on answers a message to a group,
  so a question they asked before one stays on its own row; written to alone, it's listed on its
  own again, and a later group message moves it under that group. Done, on the group or on
  them, doesn't: what they write next is listed under the group again. An answer is what someone
  wrote after a message to a group, until the DSP next writes to them alone.
- **Open:** its faces, name and how many it went to, which opens its people, with how long it has
  waited and Done, while anything under it waits; the DSP's messages to it under their sender,
  each with how far it has gone (Sending 5 of 8, Sent to 8, Sent to 7 of 8); each answer under
  its writer's name, which, with their photo, opens their own conversation, as Reply does, shown
  as an answer is pointed at or reached by the keyboard; and Message all, to the same people. In
  someone's own conversation, a message to a group carries its name beside its sender's, which
  opens it.
- **Its people,** in the profile's place: the group, who first sent it and when, and each of its
  people with what they do and whether they replied, or its latest message is sending, was sent
  or wasn't and why (Not sent: too late, Slack needs updating, Slack disconnected, Communications
  off in Slack, no longer in Slack, Slack refused it), each opening their own conversation.

## Messages in

- **Kept:** each message a person writes to the app anew, a file they share there, or a reply
  they write in one of its threads, with its text, when it was written, Slack's IDs of it, its
  workspace and its writer, and its photos and files. Never the app's own posts, another bot's,
  or Slackbot's. Slack's markup is kept as it reads: a link by what it shows, with its address
  when that's another, a mention by the member's name, a channel by its name.
- **Conversations:** one with each member of each workspace, in the direct message the app has
  with them there, under the name their Slack profile last gave them; who they are, their photo
  and their role, or title when they have one, come from Driver Match (`directory`), by their
  Slack ID.
- **Edits:** a member editing a message in Slack replaces its text, and marks it edited; nothing
  of what it said before is kept, logged or read. Slack changing a message itself, as to show a
  link's preview, isn't an edit. An edit Slack tells of before its message, as when keeping the
  message failed and is tried again, keeps the message as edited, and the message told of after
  doesn't change it back. A message kept is edited while Communications works there, though it
  was sent before Communications was last switched on.
- **Told of:** what Slack tells of is kept once Slack has its answer; keeping it is tried again a
  second, five and thirty seconds after it fails, then left to catching up, the failure logged as
  `communications_heard_failed` with its DSP and code alone.
- **Deletions:** a message deleted in Slack stays.
- **Photos and files:** downloaded from Slack into Dispatch, kept by their bytes' SHA-256 as
  every picture and file is ([Data and upkeep](#data-and-upkeep)), so they outlive Slack. Each is tried
  at once, then, when it fails, after a minute, five, fifteen, an hour, three, six and twelve,
  and given up after that; it waits, uncounted, while the workspace's admin hasn't approved
  reading files or the workspace needs reconnecting. One of a workspace the DSP no longer has
  is tried again later each time, in case it's connected again, and then given up. Only files Slack hosts are downloaded, up to 100 MB; a larger one, or a link to
  elsewhere, keeps its name, type and size alone.
- **Never before it was on:** a message is kept only if it was sent once Slack, Communications
  and its integration in that workspace were all on, the latest of the three
  (`feature_on_since`, and Slack's switch record or the workspace's connection): one Slack tells
  of late, or catching up finds, sent before then is left. Slack's other answer, while it's off,
  keeps nothing.
- **Catching up:** as the server starts, and hourly, each direct message the app has in each
  workspace where it's on, once its admin approved reading them, is read from Slack
  (`conversations.history`, and `conversations.replies` for the threads in it) from when it was
  last read, or from when its messages may first be kept, never earlier, so what Dispatch didn't
  hear of while it couldn't hear Slack is kept as Slack would have told of it, once. What was
  written in the week before is read again for any edit since, or reply in its threads. Slack
  lists the latest first, so a direct message with more since then than a pass reads (2,000
  messages) goes on, the next pass, from the oldest it read, until it's back down. Its direct
  messages are those Dispatch knows, and each other Slack lists the app as having
  (`users.conversations`, with `im:read`) once the workspace's admin has approved that.
- **Kept until the DSP is deleted:** disconnecting a workspace, or Slack removing the app from
  it, keeps its conversations, each message with who sent it and its photos and files; its
  integration's `forget` lets go only of the conversations holding no message.

## GIFs

A part of Communications switched on its own, `communications.gifs` (GIFs), off until the
platform owner switches it on for a DSP; then, while the server has its KLIPY API key
(`KLIPY_API_KEY`), **Choose GIF** under the plus opens KLIPY's GIFs where the emoji picker opens:
trending ones until something is typed in **Search KLIPY**, then those KLIPY finds for it, in
KLIPY's order, with KLIPY's content filter at `medium`. The one chosen stands in the box as a photo
does, in place of photos, with what's written; once it's sent, KLIPY is told it was shared.

KLIPY's terms, as KLIPY approved them for Dispatch: the browser searches KLIPY and shows each GIF
from KLIPY's addresses itself (`page_sources` lets them through the page's Content-Security-Policy),
and Dispatch keeps only a message's GIF's addresses (`gif`, a column of the message, the note of a
reply as it goes to Slack, and the text), never the GIF. How it goes to the person messaged is one
line, `gifs::SENDING`: `File`, as it ships, the GIF itself, which a phone fetches from KLIPY as it
sends the text, at the largest size its carrier takes ([the app](api/phone/README.md)), and Slack
shows as an image from KLIPY's address; or `Link`, KLIPY's address of it after what was written.
Either way the page shows the GIF. A text with one, sent as a file, fails `phones_app_outdated`,
"Update Relay to send GIFs", on a phone whose app is older than the first that sends them
(`RELAY_SENDS_GIFS`). In fixture mode the server stands in for KLIPY, so the dev server and tests
never call it. GIFs go in a conversation, not yet to a group.

## Messages out

- **Answering:** a message someone at the DSP writes, up to 3,000 characters, with up to ten
  photos (PNG, JPEG or GIF, up to 20 MB each), goes to the member's direct message
  (`chat.postMessage`) as written: escaped, so Slack shows what was typed.
- **Kept once, never posted twice:** each message is noted as its writer's access is checked,
  just before it goes to Slack, held for ten minutes by the try posting it, longer than its post
  takes, Slack asking it to slow down included; its post carries the note's ID as Slack's message
  metadata. Once Slack has it, it's kept, waiting its turn for the database rather than turned
  away busy, and its note cleared. One Slack may have taken that wasn't kept, as when the server
  stopped between, is looked for in the direct message (`im:history`, every page since it was
  noted) by its note's ID: by the pass every minute, once no try holds it, which keeps it if
  it's there and clears the note if not, while Communications works in its workspace, however
  long that takes; or at once when its writer sends the same message, with the same photos,
  there again, which then keeps that post rather than post a second, or, not finding it, tries
  again under the same note, so whichever try Slack took is found. Each claims the note first:
  sent again while another try of it is under way, it's refused (`communications_reply_sending`),
  and upkeep leaves a note a resend holds; a first try notes it in the same write that finds
  none, so two begun together share it. Its photos are kept while it's noted, however long, and
  no other message takes them. A note names its conversation without a foreign key, so a release
  before this one can still forget a conversation with no message; upkeep clears a note left so.
- **Under whose name:** the full name on the writer's Dispatch account, and their Slack profile
  photo (`chat:write.customize`), read from the member of the DSP's workspaces with their email
  address, the conversation's workspace first (`users.info`): Slack's own drawn photo isn't
  theirs. Each photo found, or found missing, is remembered in memory for an hour, never kept.
  With none, it goes under an image of their first and last initials, which Dispatch publishes
  at a public address on the admin's (`/communications/initials/1/<letters>.png`), because
  Slack shows a message's photo only from a web address: one or two capitals in Inter at weight
  600 on one of the dashboard's identity tones, and nothing else. `tooling/initials-glyphs.mjs`
  writes the letters' outlines (`backend/initials/glyphs.rs`) from the font the dashboard
  serves.
- **Photos:** each is uploaded to Slack as the bot (`files.getUploadURLExternal`, then
  `files.completeUploadExternal`), shared in no conversation, and shown in the message's blocks
  by Slack's ID of it, so the message keeps its writer's name and photo. Added before the message
  is sent, a photo is kept in the DSP's data as it's uploaded, its adder's alone until it's sent,
  and let go after a day unsent.
- **Messaging first:** someone at the DSP opens a direct message with any active member of a
  workspace where it's on (`conversations.open`, `im:write`): the conversation it answers isn't
  listed until a message is in it, so the page holds it until then.
- **Where it can't:** a conversation of a workspace Communications doesn't work in now, as one
  switched off there, needing reconnecting or disconnected, stays listed with its history; the
  page writes only where its workspaces say Communications works (`on`), and sending there is
  refused (`communications_off`).

## Waiting, Responded and Done

A conversation waits (`waiting`) while the member's latest message came after anyone at the DSP
answered it or marked it done, since the first of their messages after that. An answer makes it
`responded`; Done, `done`, answered without a reply, covering what was written up to the latest
message the page showed, to its `ts`, so one it hadn't shown yet, kept before the Done or after,
keeps it waiting; a new message from the member, waiting again. It follows from its messages in
the order they were written, so one caught up later, written before an answer, leaves it
answered.

## Live

The page waits on `updates`, a long poll as the dashboard's other live pages use (a `Topic`),
which answers once a change it hasn't seen is kept, or who is writing changes, or twenty seconds
pass: each conversation changed, as it is now, how many wait, and the workspaces as they are, so
the page follows one connected, updated, switched off or disconnected, and writes only where
Communications works. A conversation changes with any
of its messages, as one edited or a file kept, so a page with it open reads it again. Who at the DSP is writing in
which conversation is kept in memory alone, each for six seconds after their page last said so
(`typing`), or until they send, and shown to everyone else with Communications.

## New messages

A new message from an employee, or a text to the DSP's phones that Recents or Today's Routes
lists, alerts each
member with `communications.use` as they chose in Message display: a short chime (`sound`), and the browser's notification (`notifications`), titled
with who wrote it and saying what, as the list shows it, which brings Dispatch to the front and
opens the conversation. On any page, while Dispatch is open in a tab of their browser, in view or
not; and with it closed, or on a phone, as a push (**Pushed**, below).

- **What's new:** a message a conversation waits on that the member's tabs hadn't seen there
  (`waiting`'s `latest`), as one starting to wait or another in one waiting; never the DSP's own,
  nor one answered before it was seen. What waited before Dispatch was open there alerts no one:
  what the tabs saw is kept in the browser, by message ID, and three minutes old it's from before.
- **Once:** one of the member's tabs at the DSP alerts them, the one they last focused (Web Locks),
  the others in line for when it closes. It looks every five seconds at what waits, as the tab
  read it in the last half minute, else read again, so a message alerts within about 35 seconds.
  Taking its turn, a tab reads what the member chose again before it alerts, as they may have
  changed it in another tab, itself, even out of view, and alerts as they last chose; what it
  reads once another tab has the turn alerts no one.
- **Not in front of them:** never for the conversation open on the page, or the group it's listed
  under, while the window is in view and focused.
- **Sound** plays once the member has clicked or pressed a key in the page, as browsers ask: the
  chime is made as it plays (Web Audio), two soft notes, and rests between chimes.
- **Pushed:** a browser the member has Notifications on in, and allows them, is subscribed with its
  push service (Web Push) under the server's key, once its service worker is active, and kept as
  theirs at the DSP, with the address it was subscribed at, as each tab of theirs opens and as they
  change it; one subscribed under another key is subscribed again. A browser has one subscription
  for its address, which each DSP opened there shares, as a platform owner opens many at the
  admin's: turning Notifications off at one lets go of it there, and the browser lets go of it once
  no DSP it remembers keeping it wants it, each remembered before the server is told it keeps it.
  Each tab's changes to it take their turn among every tab's (Web Locks), reading it afresh, and one
  stops, changing nothing more, once its page moves on to another DSP or member, or they choose
  again. Each message new in what waits, as above, as the server reads it again on each change to
  the DSP's conversations, goes to each browser kept of each member who has Notifications on and may
  use Communications there then, as a session of theirs at the address they subscribed at would
  (their account, role, the DSP and its features as now), titled with who wrote it, saying what as
  the list shows it, and opening the conversation at that address; never the DSP's own. What waited
  when the server began watching the DSP goes nowhere. Pushes go apart from the change, a batch at a
  time for each DSP, eight at once across the server, each within ten seconds, one that fails tried
  once more, each push service's authorization signed once an hour; each try goes only if, as its
  turn comes, the browser is kept as it was for a member still told; a browser its push service no
  longer knows (404, 410), or of a member who may no longer be told, is let go of. Dispatch's
  service worker (`app/frontend/public/service-worker.js`) shows every push, as browsers ask,
  penalising one that shows nothing. It first asks each window of Dispatch in front there whether it
  alerts of it itself: one that does, of its own DSP, with its conversation or group open or showing
  notifications from the page, has it shown silently and closed at once, or left to the page's own
  notification of it; no such answer within a second, it stays shown. Where the worker serves the
  page, the page's notifications go through it, under the message's tag, so a push of one shown
  already takes its place. On a phone, pushes are its only notifications: Android's browser shows
  them, and an iPhone's only for Dispatch added to its Home Screen (the web app manifest,
  `app/frontend/public/assets/manifest.webmanifest`).

## The page's code

`frontend/`, by concern, each part's styles in a stylesheet beside it; `communications.css` holds
the page's layout and the variables its parts draw by.

- `page/`: the page, which section shows, what's open (in the address) and the layout; the
  section switch and the list's frame; how each section shows (`section-views.tsx`); and its keys
  (`useShortcuts.ts`).
- `inbox/`: the conversations and groups as they change (`inbox.ts`, `useInbox.ts`), the list of
  them and its rows, and the sections, each defined once (`sections.ts`).
- `conversations/`: the conversation open, its thread, composer and profile; each kind of
  conversation (`kinds.ts`: how its messages carry photos and files, and how one is known and
  started), whom it's with (`party.tsx`, `parties.tsx`: their face, profile, head, actions and
  whom a message goes to), what opens one (`target.ts`), and what's written in each and not sent
  (`drafts.ts`).
- `slack/`, with `groups/`: Slack's contacts and workspaces, New message, Copy chat link and
  setting Slack up; messaging a group.
- `texting/`: Recents' text contacts, a number's actions and Save contact (Add to driver or a
  name), phones offline, and the banner of phones down on every page (`PhonesDown.tsx`);
  `texting/phone/`, the phones' card on DSP Connections, Add phone and Rename phone.
- `todays-routes/`: Today's Routes' read, list and board, a route's facts, a driver's workdays,
  Pick driver and No work phone.
- `shared/`: what any of them draws with: faces, a list's rows and headings, a contact's row, a
  message's body and runs, a thread's frame, a pane's head, the side panel, how the page words
  things (`format.ts`), and what each key does (`shortcuts.ts`).
- `replies/`: the DSP's quick replies, in the composer and in Settings.
- `alerts/` and `display/`: new messages' alerts on every page, and pushes to the browser
  (`push.ts`); and Message display.

Where to add:

- **A section:** its model in `inbox/sections.ts` (whether the DSP has it, which conversations are
  its own, whether it lists every section's, how it pages and where it's read and searched) and
  its view in `page/section-views.tsx` (label, icon, contacts, its own rows, what's beside Search,
  a board), both by its ID, which
  members' browsers keep once it ships. A section another feature supplies needs a slot core owns.
- **A kind of conversation:** its kind in the API, then `conversations/kinds.ts`, its party in
  `party.tsx` and `parties.tsx`, its section's `holds`, and its chip in `ElsewhereChips.tsx`.
- **What the page says of the phones:** as phones offline, in `texting/`; managing them, in
  `texting/phone/`. The phone side itself stays behind Communications' API.
- **How new messages alert:** `alerts/`.

## What it reads of other features

The profile, the groups a group message can go to, and texts' filing read what other features
expose, each only while the feature offering it is on for the DSP, for anyone with
`communications.use`, who needs no permission of theirs:

- **`directory`** (Driver Match): who the member is, by their Slack ID, and their phone numbers,
  their default first, the latest added to theirs, else the first their sources have, and their sources'
  first email address (Driver Profiles' roster, Slack's profile); and who texts from a number,
  by any of their phone numbers.
- **`live_route_data`** (Routes): the live day, its routes (`day`, and a driver's, `routes`) and
  each driver's latest work phone today (`day_phones`); and from `driver_routes`, a saved day's
  work phones, whether a number was ever a work phone, and each earlier workday's route of a
  driver's conversation. It hears `routes.data_changed`, and takes the longest driver ID it
  accepts from Routes' contract (`MOST_ID_LENGTH`).
- **`driver_routes`** (Routes): their route today, the day it is in the DSP's timezone, read for
  their IDs of its source, today alone, as live route data last read it while it holds today,
  else as Routes last read it: its code, van, wave, stops and pace, with the work phone they
  have. Of several, the one with the most stops, as a rescue they drove too has fewer. And for a
  group
  message, every driver's routes today, read whole (`day`), mapped to people by those IDs, then
  to their Slack accounts as the profile does in reverse: who's on a route, and in which wave.
- **`amazon_fleet`** (Vans): what the DSP calls that route's van, by its VIN alone.

## Finding people

New message and Contacts list every active member of the DSP's workspaces where it's on, or of
one of them, with their photo, role and conversation, by name, up to 2,000. A name typed,
misspelt or not, finds them, and the list's conversations, the closest first: each word typed is
matched to the word of a name it starts, or shares its first letter and most of its letters with
in order; case and accents don't count. The letters that matched come with each, as runs of the
name's UTF-16 code units, for the page to mark.

## API

Behind `communications.use`:

- `GET /api/dsp/communications`, with `team=<id>` for one workspace's answered conversations
  alone: the conversations waiting, the longest waiting first, and the groups waiting, then the
  fifty latest answered conversations and groups of each section, Slack's, the numbers' and,
  with live route data, the work ones of any day, and whether older are left of each (`more`,
  `moreTexts`, `moreWork`) (one listed under a group is the group's, as its `groupId` says),
  with the DSP's workspaces (whether it works in each, the address of the app's Messages tab
  there, `chatLink`, whether, on there, it needs updating for what Communications uses,
  `needsUpdate`, and the version of its logo, `logo`, none without one), who is typing, each
  with their person and photo, and the latest change (`revision`).
- `GET …/quick-replies`: the DSP's quick replies, in their order, and their `revision`;
  `POST …/quick-replies`, `{replies, revision}`: keeps them, each trimmed, as the revision they
  were read at, refused past 30, one empty or over 500 characters (`invalid_input`), or once
  another save passed it (409 `settings_changed_reload_before_saving`).
- `GET …/waiting`: how many conversations wait, and since when each has, for the sidebar; the
  latest message from whom each is with (`latest`: the conversation, the group it's listed under,
  their name and the message), for alerts; and the DSP's phones an app holds, each online or not,
  since when, and whether it can't send texts (`phones`), for the banner of phones down.
- `GET …/updates?after=<revision>&typing=<version>`: the long poll, with the workspaces as they
  are.
- `GET …/conversations?q=<name>`, or `?before=<lastAt>:<id>`, with `team=<id>` for one
  workspace's alone, or `kind=number` for the numbers' (`q` a name or a number's digits), or
  `kind=work` for work conversations: the fifty conversations found closest to a name, or the
  answered ones before the last shown.
- `GET …/conversations/{id}`, `?before=<ts>`: a conversation and its fifty latest messages, or
  those before one, in the order written; by text, `ts` is `<at>:<id>`; with whom it's with's
  conversations of each kind (`elsewhere`), for a driver's, their route `today` and each of its
  days' route and van (`workdays`), for one with someone's own number or in Slack, the route
  they drive today (`today`, its van in the head), and what a message written in it may carry
  (`limits`: its text's characters, photos, each photo's bytes and their kinds; Slack's own, or
  as the phones hold a text to).
- `GET …/conversations/{id}/profile`: what the profile shows of whom it's with, a member of a
  workspace or a driver, besides the name, role and photo the conversation has: their route
  `today` (`route`, `van`, `waveAt`, `stops`, `stopsDone`, `pace`, and the work phone they have,
  `phone`), their phone numbers (`phones`, their default first), `email`, and the workspace's
  name (`slack`), each none when unknown.
- `GET …/routes`: Today's Routes: the live day, its drivers by name, each with their route
  `today` and work conversation, and the work phones nobody had that texted today.
- `POST …/conversations/{id}/driver` (`driverId`): Pick driver, for a work phone nobody has: the
  driver's conversation.
- `POST …/conversations` (`teamId`, `userId`): opens a direct message with a member.
- `POST …/conversations/{id}/messages` (`text`, `photos`): sends an answer, its text and photos, a
  Slack one's added by `…/uploads`, a text's pictures by `…/texts/uploads`, as `limits` says; by
  text, refused as `phones_none` while no phone is paired. Answers the message and the
  conversation it's in now (`conversation`): by text, another once the number changed hands as it
  went, which the page then opens. The member's access is checked again before it answers: once it ended as
  the answer went, what went is kept, and the answer refused.
- `POST …/conversations/{id}/messages/{message}/retry`: sends again a text Dispatch sent that
  failed.
- `POST …/conversations/{id}/done` (`seen`, the place of the latest message the page showed, as
  `before` names it): covering only what was written up to it, so a message the page hadn't shown
  still waits; without it, everything. And `…/typing`, which says the member is writing.
- `POST …/conversations/{id}/contact` (`name`), `…/block` and `…/unblock`: a number's
  conversation's, as the number's.
- `POST …/texts/conversations` (`number`, or `driverId` for one of today's drivers): opens the
  conversation a text to the number written now is filed in, as filing decides (a driver's while
  it's their work phone), or the driver's, to text them first.
- `POST …/texts/uploads?name=&type=`: a picture for a text not sent yet, as its bytes, kept by
  the phone side.
- `GET …/texts/contacts?q=<name or digits>`: whom the DSP can text, and the numbers blocked.
- `GET …/texts/files/{id}`: a text's picture or file, as the phone side keeps it.
- `GET …/texts/files/{id}/preview`: the picture of the first frame of a text's video its phone
  keeps.
- `POST …/texts/files/{id}/fetch`: asks the phone of a text's video it keeps for its bytes, the
  member's access checked again in the write that asks, and waits up to 40 seconds
  (`FETCH_WAIT_MS`) for them, answering `{ kept }`; played again while the phone sends it, it waits
  on the same. Refused `phones_offline` (503) while the phone is, and `phones_file_gone` (410) once
  it has it no more. The page asks it each time a video's played, so one the server let go of is
  asked of its phone again.
- `GET …/contacts?q=<name>&team=<id>`: whom the DSP can message, every one or those found
  by a name, in every workspace or one, each with their conversation, if any, since when it
  waits (`waitingSince`) and when it was last written in (`lastAt`).
- `GET …/groups/choices`: whom a group message can go to, each person once with their accounts
  (`people`), and the groups to choose from, each holding someone (`today`, `roles`,
  `workspaces`), by their people's keys; and what a message to a group may carry (`limits`, a
  Slack message's).
- `POST …/groups` (`name`, `people`, each `teamId` and `userId`, `text`, `photos`): messages a
  group, sent in the background.
- `GET …/groups/{id}`, `?before=<at>:<id>`: a group and its fifty latest messages and answers, or
  those before one, each of the DSP's with how far it has gone (`progress`), and what a message
  to its people may carry (`limits`, a Slack message's). In the order written: within a
  millisecond, the DSP's first, then the answers by Slack's `ts`, then by ID, the order its paging
  and Done go by too.
- `GET …/groups/{id}/people`: who it went to, who first sent it and when, and where it stands
  with each.
- `POST …/groups/{id}/messages` (`text`, `photos`), to the same people; `POST …/groups/{id}/done`
  (`seen`, the latest of its messages and answers the page showed, `<at>:<id>` as `before` names
  one), for each conversation listed under it that waits, covering only what was written up to
  it.
- `GET …/push`: the key the server's pushes are signed with, which a browser subscribes with
  (`key`), none while it pushes nothing; `POST …/push` (`endpoint`, `keys`: `p256dh`, `auth`)
  keeps the browser a subscription names as the member's, subscribed at the address the request
  came to, at a push service browsers use (Chrome's, Firefox's, Safari's or Edge's) alone, and
  `POST …/push/remove` (`endpoint`) lets go of the member's own at the DSP.
- `POST …/uploads?name=&type=`: a photo for a message not sent yet, as its bytes.
- `GET …/files/{id}`: a photo or file attached to a message.
- `GET …/workspaces/{team}/logo?v=<logo>`: a workspace's logo, the copy Slack keeps of it, read
  through Slack's `kit`; the browser keeps it until its version changes.

Behind `phones.manage` or `connections.manage`, while Communications is on (`not_found`
otherwise), the phones' card's:

- `GET /api/dsp/communications/phones`: the DSP's phones, those removed left out, in the order
  paired, each with what needs attention on it (`health`), none from an app that never said.
- `POST …/phones/pairing`: a new code to pair the next phone, in place of any shown before, with
  its picture and when it expires.
- `GET …/phones/pairing/{id}`: how that code stands: waiting, paired, replaced or expired.
- `POST …/phones/{id}/rename` (`name`, up to 60 characters), and `…/remove`, its access checked
  again as it's removed.

Public, on the admin's address: `GET /communications/initials/1/{letters}.png`. Public to the
server, at a DSP's own address, by a phone's key, the phones' own:
`/api/communications/phone/{pair,outbox,texts,statuses,files/{id},files/{id}/preview,files/{id}/gone}`, as their
[protocol reference](api/phone/README.md) says.

## Data and upkeep

- **Data:** in each DSP's own database, `communications_conversations` (with the group each is
  listed under, and how far its latest Done covered: `done_at` and `done_ts`, migration 13),
  `communications_messages` (with the group each went to or answers), `communications_files`
  (each photo and file, and downloading it; a group message's own photos too),
  `communications_changes` (each change to a conversation or a group, the latest thousand,
  for `updates`); `communications_groups` (each group, its name, who made it and whether it
  waits), `communications_group_people` (whom each went to, by workspace and member ID, under
  their name then), `communications_group_messages` (each message to a group, its sender's name
  and the letters of their initials), `communications_deliveries` (each going to each person:
  waiting, sent with their message, or not sent and why), `communications_group_uploads` (a
  group message's photos as uploaded to each workspace) and `communications_replies` (each answer
  noted on its way to Slack, until it's kept or found not to be there; migration 15); the files'
  bytes by their SHA-256, `data/communications/files/` until they're in the object store (below).
  By text: `communications_text_conversations`
  (each a driver's, a work phone's nobody had, or a number's), on each text's own row
  (`phones_texts`, below) the conversation it's filed in, none while it waits to be, the name its
  sender was shown by, whether its number was blocked as it came and the Done that covered it
  (migration 9, which copied each from `communications_texts`, since dropped),
  `communications_contacts` (the names saved for numbers, by whom and when, the latest alone:
  the audit log keeps each),
  `communications_blocked` (the numbers blocked, by whom and when, until unblocked: the audit log
  keeps each) and `communications_picked`
  (the driver picked for a work phone on a day, by whom and when); and, from migration 17,
  `communications_refiling` (each day still to be filed again, and since when),
  `communications_live_days` (each day live route data was seen holding, when it was first seen
  not to, and whether the pages heard the two hours after end) and, on each text waiting to be
  filed, since when it couldn't be placed (`phones_texts.unfiled_since`), and on each Dispatch
  sent, the conversation it was written in (`phones_texts.queued_in`); and, from migration 19,
  the GIF of KLIPY's a message carries, by its addresses (`gif`, on `communications_messages`,
  `communications_replies` and `phones_texts`). The DSP's quick replies,
  with their revision, in its `settings` table under `communications.quick_replies`. For pushes,
  `communications_push` (each browser a member is told in, by where its pushes go, with the keys
  they're encrypted for and the address it was subscribed at: migration 18), kept until it's let
  go of. The phone side's, as Phones made them before Communications adopted them (its migrations
  7 and 8, which find them made on a database Phones migrated, and leave Phones' own records of
  its migrations as they were): `phones_devices` (each phone, by its key's SHA-256, and how it last
  said it stands, `health`, migration 14), `phones_pairings` (the code that pairs the
  next, by its SHA-256, with its ID and when a newer one replaced it), `phones_texts` (each text,
  with its phone, direction, origin, the other party's number, how one Dispatch sent went, each
  attempt at it, and its latest change), `phones_files` (each text's pictures and files, and those
  added for one not sent yet) and `phones_change` (the last change given); and its own,
  `phones_attempts` (each earlier attempt at a text Dispatch sent, by the phone it was given to,
  kept as Retry queues the next: migration 12), and, on `phones_files`, a video its phone keeps,
  with the picture of its first frame, when it was last asked for and its phone told, when its
  bytes came, and when its phone said it has it no more (migration 16); the files' bytes by their
  SHA-256, `data/phones/files/` until they're in the object store. In either folder (`bytes`),
  bytes arrive beside those kept, and take their place, and are let go, only under the
  platform's write lock with the files that keep them. Kept until the DSP is deleted, as the rest.
- **Where pictures and files are kept:** with an object store, core's bucket of Cloudflare R2
  (`db::objects`, one for each environment), it keeps them for good, each by
  `dsps/<dsp>/<phones|communications>/<sha256>`, a key the server makes of the DSP it checked,
  never of what a request names. New bytes wait in the DSP's data, which backups keep, until
  upkeep has copied them there. Until `R2_ONLY` is set (core's), they stay in the DSP's data
  too, marked copied in its cache by the bucket that has them
  (`cache/<phones|communications>/copied-<bucket>/`, named for its address and name), so a
  release that reads only the disk, as one a rollback returns to, finds every one, and another
  bucket in its place has them copied to it again; once it's set, the copy the bucket in place
  has becomes the server's cached one, in the DSP's cache (`cache/<phones|communications>/files/`),
  which backups leave out. Bytes are read from the DSP's data, opened before the cache is looked
  at, else its cache, else the object store, cached again as they come down, or uncached when
  the cache can't be used, a folder that can't be used passed over, so bytes the server has
  never wait on R2, nor on its cache; reading a cached copy or a preview marks it read. Those
  asking for a preview at once share bringing its picture down. The server keeps every DSP's
  cached copies and previews within its limit, `PICTURE_CACHE_MB` megabytes (10 GB unless the
  environment says), letting go of those read longest ago first; never of bytes in a DSP's data. Without an object store (none of `R2_ACCOUNT_ID`, `R2_BUCKET`,
  `R2_ACCESS_KEY_ID` and `R2_SECRET_ACCESS_KEY` set, as in development and tests), bytes stay in
  the DSP's data, as they always did.
- **Upkeep:** catching up hourly and as the server starts; downloading what's due, keeping
  answers that went to Slack unkept, sending what's due to go to groups' people and giving up
  those too late, filing texts waiting to be filed, those waiting ten minutes under their number,
  filing the days noted and today's again where they belong now, keeping when the live day ends,
  failing the texts still waiting on an offline phone as they expire, and letting go of the bytes
  of videos their phone sent an hour before, every minute, each failure logged with its DSP; and
  hourly, in each DSP with Slack connected, letting go of
  photos added and never sent within a day, then of bytes no file keeps, as a failure once in
  place or a removal that failed leaves, and of bytes left arriving an hour, as a restart leaves,
  each on its own: one that fails, logged as `communications_tidy_failed` with its DSP and job,
  leaves the rest to go, and is tried again the next hour. And the phone side's, hourly, in each DSP with
  Communications on: letting go of pictures added and never sent within a day; of bytes no file
  keeps, as bytes sent again in place of others and a removal that failed leave, and of bytes left
  arriving an hour, as a restart leaves, each on its own; and of codes expired. Each of these jobs
  runs on its own too: one that fails, logged as `phones_tidy_failed` with its DSP and job, leaves
  the rest to run, and is tried again the next hour. Either's letting go of bytes lets go of their
  cached copies and previews too, and of previews left being made an hour, as a restart leaves.
  And hourly, with an object store, in each DSP with Communications on, for each store of bytes
  (`copies`): letting go of what the object store keeps that no file does, once a day, as listing
  it is paid for by the page; then copying what waits in the DSP's data to it, a hundred at a
  time, keeping the cache within its limit after each hundred, and, once `R2_ONLY` is set, making
  each the cached copy. And hourly, in a task of its own, so copying never holds it up: keeping
  the cache within its limit. Each on its own: one that fails, logged as
  `communications_copies_failed` with its DSP, store and job, is tried again the next hour, its
  bytes still waiting in the DSP's data, and R2 being down or slow holds up no text or message.
  And every minute, while the server pushes, each DSP with Communications on and a browser kept
  is watched for what to push, as after a restart; one whose pushes fail is logged as
  `communications_push_failed` with its DSP, and one a push service refused twice as
  `communications_push_refused` with its status, never its address.
- **Fixture mode** never calls Slack: Slack's fixture answers what Slack itself calls, and
  `integrations/messages/fixture.rs` what only Communications does. Each member's direct
  message with the app is `D` and the rest of their ID, and holds what the app posted there, as
  Slack lists it; Jordan Ellis's holds what they wrote while Dispatch wasn't listening, for
  catching up: a message, a photo, a question with a reply in its thread, the app's own answer,
  and one from before 2026. Nor a push service: `backend/push/fixture.rs` keeps what would be
  pushed for the server's run; an address whose last part begins `gone` or `missing` answers as
  one its browser let go of, and `busy` refuses its first push.
- **Server environment:** `VAPID_PRIVATE_KEY` (`DISPATCH_DEV_…` on Dev, `DISPATCH_PRODUCTION_…`
  on Production): the private key of the server's own key pair its pushes are signed with, 32
  bytes in base64url, as `npx web-push generate-vapid-keys` prints one; its public key follows from
  it. Without one the server pushes nothing, but in fixture mode, which makes a pair for its run;
  one that isn't a key is logged as `communications_push_key_invalid`, and nothing is pushed.
  `PICTURE_CACHE_MB`, how many megabytes of cached pictures and files the server keeps:
  `DISPATCH_DEV_PICTURE_CACHE_MB` on Dev and its previews, `DISPATCH_PRODUCTION_PICTURE_CACHE_MB`
  on Production; 10 GB unless it's set. The object store, and `R2_ONLY`, are core's.
