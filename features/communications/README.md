# Communications

The DSP's direct messages with the members of its Slack workspaces, and the texts its dispatch
phones relay. Employees write to the DSP's Dispatch app in Slack, in its Messages tab; anyone
texts the DSP's phones; anyone at the DSP whose role gives them Communications reads each
conversation on the Communications page and answers it there. A Slack answer reaches the
employee's direct message under the full name and photo of whoever wrote it; a text answer goes
from the DSP's phone as written. Slack's part works on Slack's own foundation for direct
messages: while it's on in a workspace, Slack no longer answers what's written to the app that
DSP Messaging is unavailable. Texts come through whatever offers `texting` ([Texts](#texts)):
drivers' work phones on Today's Routes, and every other number in Messaging.

- **Switch:** `communications`, off for every DSP until the platform owner switches it on. Its
  code depends on Slack (`depends_on: ["slack"]`), which a build without Slack leaves it out with;
  for a DSP, it needs neither: without Slack on, it hears nothing of Slack, and without texting,
  nothing of texts.
- **Permissions:** `communications.use` (Use Communications) covers reading the DSP's
  conversations, who each is with, and answering them. Owners hold it; another role holds it once
  the DSP's owner turns it on. Every route is behind it, but the image of someone's initials.
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
  Settings: its text size, line spacing and spacing; the messages across the conversation or in
  a centred column, bubbles or plain, your team's side, bubbles' shape and width, each message's
  time or each run's or on hover, photos and your team's names by them; your team's and the
  employees' colours; the list's width, rows, preview lines, photos and waiting times; the
  profile beside or over the conversation, its width, its slide, and whether it opens with each
  conversation; and whether Enter sends. A phone (a touch screen whose shorter side is under
  500px) has its own of those that apply on one, `phone_<choice>`: no layout, list or profile
  width, profile beside or kept open, or times on hover; and Enter starts a line there to start.
  Then, on any screen, how a new message alerts them, under New messages: `sound`, on to start,
  and `notifications`, off, which turning on asks the browser for, staying off if it refuses, and
  which shows on only in a browser that has allowed it; left out where the browser can't show
  one, as a phone's.
- **On every page:** `communications.alerts`, a watcher (`frontend/alerts/`), for those with
  `communications.use`, alerting them to new messages ([New messages](#new-messages)).

## The page

- **Communications** in the sidebar, for those with `communications.use`, with how many
  conversations wait beside it, Slack's and the numbers' texts (`waiting`, read every 30 seconds while the page is closed, and
  as the number changes while it's open). The page fills the window, however wide (`fills`),
  the list beside the conversation open, each scrolling on its own, and everything on it grows
  with the page's own width, smoothly, from a laptop's to an ultrawide's, then stops (a
  container query on the page). On a wide page, the messages and the box under them keep to a
  centred column. Under 760 pixels of page, as on a phone, one pane at a time, edge to edge:
  the list, or the conversation open, which takes the whole page, with a way back. The
  conversation open is in the address (`?open=<id>`), and a link to one opens it, the page open
  or not.
- **Sections:** the list's, at its top while there are two or more, each with how many wait in
  it: **Today's Routes** (Routes on a phone), the drivers on a route today and their work phones,
  while the DSP texts and has live route data ([Today's Routes](#todays-routes)); **Messaging**,
  the numbers that text the DSP's phones, while it texts; and **Slack**, its workspaces' direct
  messages and the groups it messaged, while it has a workspace. The one shown is remembered in
  the browser for each member at each DSP, the first to start; a conversation or group opened, as
  from a link, shows its section.
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
- **Messages:** Waiting, the longest waiting first, each with how long it has, amber from 15
  minutes and red from 30, and a check, shown as it's pointed at or reached by the keyboard
  beside the conversation, that marks it done without a reply; then Responded, the latest
  first, each with the photo of whoever at the DSP answered last; older ones read as the list
  is scrolled to its end. Each shows who it's with, by their photo, when they last wrote, and
  what was written last, or who at the DSP is writing in it.
- **Contacts:** everyone the DSP can message, A to Z under each first letter: their photo, name
  and role, and with every workspace shown, the one they're in; how long they've waited, or
  when they last wrote. Each opens their conversation, or starts one, as New message does.
- **A conversation:** who it's with, the workspace it's in while there are several, and, while
  it waits, how long and Done; its messages under the day each was written, the member's beside
  their photo, the DSP's under the name and photo of whoever sent them, each edited in Slack
  marked Edited, a reply in one of its threads in its place among them, photos shown and other
  files to download, or why one isn't kept; older ones read as it's scrolled to its start; and
  those at the DSP writing in it now. Writing in it: text, with photos attached (the paperclip),
  as much as one message carries as the conversation's read says (`limits`), which what's refused
  says too, and nothing until it's read; Enter to send; while writing, teammates see it. Photos
  show by Driver Match's person for each (`/api/dsp/people/{code}/photo`), never Slack's own
  addresses.
- **The profile** of whom a conversation is with opens from who it's with, above it, or their
  photo by their messages, over the conversation, and closes with its X, Escape, or a press
  anywhere else; from 1,500 pixels of page, it opens beside the conversation instead, a column
  of its own covering nothing, and closes with its X or Escape. Its X and Escape put focus back
  on what opened it. Their photo, name and role; Call, while they have a phone number; Today:
  their route on the DSP's own day, its van, when its wave starts, how many stops it has and are
  done, and its pace; and while live route data holds today, the work phone they hold;
  Contact: their phone number, email address and the workspace the conversation is in. Each is
  left out when nothing knows it, and Today when they have no route today. No feature has a page
  of a person to open, so it has no Full profile.
- **New message** finds whom to write to by name, misspelt or not, and opens their
  conversation, or opens a direct message with them, kept at the top of the list until it holds
  a message. With nothing typed, Message a group comes first.
- **Display** opens the member's own Message display in Settings, which the page looks as, with
  either section.
- **Copy chat link** copies the address that opens the app's Messages tab in a workspace where
  Communications works, chosen from a menu with several, for employees to write to the DSP.
  It and New message show only with such a workspace.
- **Setting Slack up**, for those who also hold `connections.manage`, which connects and updates
  the DSP's workspaces: while the DSP has none, and doesn't text, Connect Slack takes the inbox's
  place; while a workspace where Communications is on hasn't approved all it uses
  (`needsUpdate`), Update Slack covers the page while it shows Slack, not the sidebar, with no
  way to close it but to Messaging, while the DSP texts; each such workspace with Update and Copy
  link (its own install link, for whoever approves apps in that Slack), until an update brings
  the workspaces with none needing it, within twenty seconds. Connecting or updating from here
  comes back here from Slack, which finishes it. Everyone else sees the page as it is, empty
  without Slack.

## Today's Routes

The drivers on a route today, while live route data holds today, and the work phones they text
from. At the stop, as live route data empties, it lists nobody (No routes), and the conversations
stay; the next day's drivers fill it as its live reads start.

- **Who:** each driver its routes name today, with their route as last read live (of several
  itineraries, the one with the most stops), and their work conversation once they have one;
  each driver holding a work phone today but gone from the day's list, once they texted today;
  and each work phone nobody held that texted today, as itself (Work phone 5521).
- **The list:** Waiting first, the longest waiting first; then each wave by when it starts, its
  drivers by name; then the drivers with no wave (No wave); then the work phones nobody held (No
  driver), the latest written in first. Each row as a conversation's in the other sections, and
  under it the driver's route code, the work phone they hold now by its last four digits, a bar
  of the stops done, `98/168`, in the colour of its pace, and its pace but on time: Behind, At
  risk (Amazon's `BEHIND`, `AT_RISK`), or Done, every stop of it. A driver without a conversation
  yet opens one, to text them first.
- **The board:** beside Search, the list or the board, remembered in the browser for each member
  at each DSP, and not offered one pane at a time: the same groups as a table across the page,
  each driver a line of route, wave, van, work phone, stops and pace, and the last text with how
  long it has waited or when. It shows while nothing is open: one chosen there, or opened by a
  link or a chip, opens in the list, and the board comes back once nothing is; choosing the board
  leaves what's open.
- **Search** finds work conversations, across days, by a driver's name or the digits of any work
  phone they texted on.
- **Fresh:** read when it shows, again every minute while the page is in view
  (`ROUTES_READ_MS`), as live reads say nothing as they come, at once when an update changes a
  work conversation, and as the page comes back into view or the browser back online, a read that
  failed tried again, as the dashboard's reads are. Loading until it's first read, never No routes
  before then; a read that fails says why in Today's Routes, the list or the board, and nowhere
  else.
- **A driver's conversation:** under its head, their route code, van and the work phone they hold
  now, and how long it has waited; chips to their conversation of each kind, Work (by the work
  phone they hold), Messaging (by their own number) and Slack, each marked with a dot while it
  waits, opening it or starting it ([Elsewhere](#elsewhere)). Each workday begins with a line
  across it, Today · Sun, Oct 25, Yesterday, or the date, with that day's route and van and the
  work phone its first text went through; earlier days dimmed; and where they texted from
  another work phone that day, Switched to 7643. With no work phone now (one claimed by another
  driver, never claimed, or once the day stopped), No work phone takes the box's place, with
  chips to their other conversations; what was being written in it stays, its pictures too, and
  comes back with a work phone.
- **A work phone nobody held:** No driver under its name, and **Pick driver**: one of today's
  drivers, found by name or route, each with their route and wave; picked, the phone's texts of
  today are theirs ([Texts](#texts)), and their conversation opens. Escape closes it, focus going
  back to Pick driver.
- **The profile** of a driver, from their name or photo: the ways to reach them, as the chips;
  Today: route, van, wave, stops with their bar, pace, and the work phone with since when, as the
  conversation's head shows them, one source for both: Today's Routes' latest read of the driver
  while it shows, else the conversation's own, each kept up as the page's are, and none once
  Today's Routes says the day is over; Contact: their phone number, email address and Slack
  workspace.

## Elsewhere

A person's conversation of each kind, which the page goes between: their work conversation, as a
driver who held the DSP's work phones; the one by text with their own number, the latest written
in of Driver Match's numbers for them, else the first; and their Slack one, in a workspace where
Communications works, the latest written in, else their first account there. Shown, as chips,
on each of them while they have two or more, each waiting as the page's list has it, so its dot
follows each update, and one started since is found by whom it's with. A driver holding a work
phone now has the work conversation of the ID holding it, as Driver Match may know them by
several, started once they have none there; else the latest of any of their IDs.

## Messaging

The numbers that text the DSP's phones: drivers' own phones and any other number, from people
who may or may not work for the DSP.

- **Who:** each number's conversation is with the person Driver Match has that phone number for,
  by its last ten digits (of several, one who hasn't left first, then by name), by their name and
  photo; else by the name the DSP saved for it; else the number, `(425) 555-0199`, by a phone.
- **Messages:** as Slack's, Waiting then Responded, with Search finding them by name, misspelt or
  not, or by a number's digits; a blocked number's isn't listed.
- **Contacts:** everyone Driver Match has a phone number for who hasn't left, by their first, and
  every number the DSP saved a name for, each number once, A to Z, each with its number and how
  long it has waited or when it was last written in, opening its conversation or starting one;
  found by a name or digits typed. Then **Blocked**, each number blocked with Unblock.
- **A conversation:** who it's with, their number while it isn't their name, and while it waits
  how long and Done; **Save contact** (Edit contact once saved), while Driver Match knows no one
  by the number: a name, up to 80 characters, shared by everyone at the DSP, never left changed
  and unsaved without Save changes? asking to save or discard it, and closed by Escape with focus
  back on its button; its menu, **Copy
  number** and **Block** or **Unblock**. A number blocked leaves Messages and the counts, and
  what it sends while blocked never makes it wait, then or once unblocked; its texts are still
  kept. Its texts as Slack's messages show, the other party's by their face or a phone; the DSP's
  under who wrote each, or the phone's name and mark for one sent from the phone's own messaging
  app; each picture shown, as the phone keeps it. Under a text the DSP sent, the last of a run or
  one that failed: Sending, Sent, Delivered, or Not sent with **Retry**.
- **Writing:** text, with pictures (PNG, JPEG or GIF, by their stated type), as much as one text
  carries as whatever offers `texting` says (its `limits()`: Phones', 10,000 characters and ten
  pictures of up to 20 MB each), Enter to send, the number's last four digits beside it while the
  name doesn't say it; teammates see who's writing, as in Slack's. As in a driver's work
  conversation.

## Texts

What the DSP's dispatch phones relay, through `texting` (`dispatch-phones-contract`), offered by
Phones, while it's on for the DSP: its phones, the texts changed after a point, sending one, and
sending again one that failed.

- **Filed:** as Phones says texts changed (`phones.texts_changed`), and every minute, in case a
  hearing was missed or failed, or Communications came on after texts did, each text changed
  since the last filed is kept (`communications_texts`, by Phones' ID), with its phone, direction,
  number, body, when it was written, who at the DSP sent it, how it went, and its pictures as
  Phones has them, in a conversation (`communications_text_conversations`):
  - **A work phone's**, a driver's: a number `+1` and ten digits, held as Routes' work phones are,
    is filed under whoever held it as it was written, by their ID of the source naming who held
    the DSP's work phones: live route data's holds while its live day is the text's day
    (`live_route_data`), else that day's saved holds (`driver_routes`). Of the number's holds that
    day, the one covering when it was written; written before any, the day's first holder, as
    it came before the day's first live read or before they signed in; written once nobody held
    it, the work phone's own, with no driver. A number nobody held that day but ever held as one
    (`was_work_phone`) is a work phone too, nobody's. A driver's conversation is theirs across
    days, whichever phone they held. Where someone at the DSP picked a driver for the phone that
    day (Pick driver, `communications_picked`), a text no hold covers is that driver's, before
    the day's first holder. Both are listed in Today's Routes.
  - **Any other number's**, Messaging's, by the number.
  - **While Routes is off** for the DSP, nothing says whether a number is a work phone: a text
    kept already stays where it was filed, and one that comes is placed under its number until
    holds can be read. A text is placed as it first comes; how it went, or its pictures coming,
    never moves it.
- **Filed again:** who held which phone settles as the day goes, and a day's holds may reach
  historical route data after its texts, as a text a phone relayed late; so each time a day's
  holds change (`routes.work_phones_changed`, which names the day), every text of that day, both
  ways, is filed again where it belongs now: a text that came before the day's first live read
  goes to whoever then holds the number. An earlier day's is filed again only while Routes keeps
  holds of it, so a day it emptied unsaved keeps its texts where its holds had them. Today's are
  filed again every minute too, so a change of holds nothing heard, as while Communications was
  off or a hearing failed, is repaired by the next pass; an earlier day's only as Routes says it
  changed, as Routes doesn't say which days had holds saved.
- **Waiting:** as a Slack conversation's: a text in after the latest text out that didn't fail,
  and not covered by a Done. Done marks each text in it covers, which stays handled wherever it's
  filed again, never handling a text it didn't cover. A failed answer leaves it waiting. A work conversation waits only on what was
  written on the live day, while live route data holds today: from the first text in on it
  after its latest answer or Done, though it came before the day's first live read; so at the
  stop, as live route data empties, none waits or counts, and the next day starts afresh. Each
  time who holds the work phones today changes, the work conversations waiting by their texts are
  noted changed, so the pages waiting on changes hear the day begin or end, and so is every
  driver's, so one open hears its driver take, return or lose one, their last hold withdrawn.
- **Answering:** what someone at the DSP writes goes as written, nothing added, through
  `texting`, from the phone that last texted that number, else the one seen last, with its
  pictures, kept by `texting` as they're added (`add_file`), the adder's alone until they go, and
  let go of (`remove_file`) should the adder's access end while their bytes come. A number's
  conversation, and a work phone's nobody held, answer their number; a driver's, the work phone
  they hold now, as live route data says, refused while they hold none
  (`communications_no_work_phone`). Kept as Phones tells of
  it, under the full name of whoever sent it (`sent_by`), taken as it's first filed, however it was
  sent: from the page, or through `texting` by anything else. Retry sends again one Dispatch sent that failed, while
  its phone is still paired. Either goes only where a text written now would: in a driver's,
  refused while they hold no work phone (`communications_no_work_phone`) or hold another
  (`communications_work_phone_changed`), as the one it went to may be another driver's since;
  checked as Phones queues it (`texting.send`'s and `texting.retry`'s condition), with the
  member's access, so a handoff, the stop or a change of their access can't come between.

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
- **Sending:** in the background, one at a time, at most one a second in each workspace, as Slack
  asks of an app posting messages (`chat:write`), in each person's direct message, opened first if
  need be (`im:write`), under the sender's full name and photo as an answer goes
  (`chat:write.customize`), its photos uploaded once to each workspace (`files:write`). Slack
  asking Dispatch to slow down (429, Retry-After) is waited out as long as it says, up to two
  minutes, and beyond that tried again two minutes later; a try that fails is tried again after a
  minute, five and fifteen, then given up; one to a workspace where Communications no longer
  works, or someone no longer in it, isn't sent at all. Each is kept until it's sent or given up,
  so sending picks up after a restart, from the pass every minute; a try that may have posted
  before the server stopped looks for its post in their direct message first (`im:history`), by
  the metadata each post carries, and keeps it rather than send a second.
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
  or wasn't, each opening their own conversation.

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
  link's preview, isn't an edit.
- **Deletions:** a message deleted in Slack stays.
- **Photos and files:** downloaded from Slack into Dispatch, kept in the DSP's own data
  (`data/communications/files/`, by their bytes' SHA-256), so they outlive Slack. Each is tried
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

## Messages out

- **Answering:** a message someone at the DSP writes, up to 3,000 characters, with up to ten
  photos (PNG, JPEG or GIF, up to 20 MB each), goes to the member's direct message
  (`chat.postMessage`) as written: escaped, so Slack shows what was typed.
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
`responded`; Done, `done`, answered without a reply; a new message from the member, waiting
again. It follows from its messages in the order they were written, so one caught up later,
written before an answer, leaves it answered.

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

A new message from an employee, or a text to the DSP's phones that Messaging or Today's Routes
lists, alerts each
member with `communications.use` as they chose in Message display: a short chime (`sound`), and the browser's notification (`notifications`), titled
with who wrote it and saying what, as the list shows it, which brings Dispatch to the front and
opens the conversation. On any page, while Dispatch is open in a tab of their browser, in view or
not; nothing reaches a browser closed, as nothing is pushed.

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
- **A phone's browser** shows notifications only from a service worker or a Home Screen app, so
  there Notifications isn't offered.

## The page's code

`frontend/`, by concern, each part's styles in a stylesheet beside it; `communications.css` holds
the page's layout and the variables its parts draw by.

- `page/`: the page, which section shows, what's open (in the address) and the layout; the
  section switch and the list's frame; and how each section shows (`section-views.tsx`).
- `inbox/`: the conversations and groups as they change (`inbox.ts`, `useInbox.ts`), the list of
  them and its rows, and the sections, each defined once (`sections.ts`).
- `conversations/`: the conversation open, its thread, composer and profile; each kind of
  conversation (`kinds.ts`: how its messages carry photos and files, and how one is known and
  started), whom it's with (`party.tsx`, `parties.tsx`: their face, profile, head, actions and
  whom a message goes to), and what opens one (`target.ts`).
- `slack/`, with `groups/`: Slack's contacts and workspaces, New message, Copy chat link and
  setting Slack up; messaging a group.
- `texting/`: Messaging's contacts, a number's actions and Save contact, and phones offline.
- `todays-routes/`: Today's Routes' read, list and board, a route's facts, a driver's workdays,
  Pick driver and No work phone.
- `shared/`: what any of them draws with: faces, a list's rows and headings, a contact's row, a
  message's body and runs, a thread's frame, a pane's head, the side panel, and how the page
  words things (`format.ts`).
- `alerts/` and `display/`: new messages' alerts on every page, and Message display.

Where to add:

- **A section:** its model in `inbox/sections.ts` (whether the DSP has it, which conversations it
  holds, how it pages and where it's read and searched) and its view in `page/section-views.tsx`
  (label, icon, contacts, its own rows, what's beside Search, a board), both by its ID, which
  members' browsers keep once it ships. A section another feature supplies needs a slot core owns.
- **A kind of conversation:** its kind in the API, then `conversations/kinds.ts`, its party in
  `party.tsx` and `parties.tsx`, its section's `holds`, and its chip in `Elsewhere.tsx`.
- **A texting provider's presentation:** what the page says of the provider, as phones offline,
  in `texting/`; the provider itself stays behind Communications' API.
- **How new messages alert:** `alerts/`.

## What it reads of other features

The profile, the groups a group message can go to, and texts read what other features expose,
each only while the feature offering it is on for the DSP, for anyone with `communications.use`,
who needs no permission of theirs:

- **`directory`** (Driver Match): who the member is, by their Slack ID, and the first phone
  number and email address their sources have for them (Driver Profiles' roster, Slack's
  profile); and who texts from a number, by any of their phone numbers.
- **`texting`** (Phones): the DSP's phones and their texts, sending and sending again, and its
  `normal`, which writes a number typed or a person's as texts' are; and it hears
  `phones.texts_changed`.
- **`live_route_data`** (Routes): the live day, its routes (`day`, and a driver's, `routes`), who
  holds which work phone now (`work_phones`) and its holds on the work phones; and from
  `driver_routes`, a saved day's holds, whether a number was ever a work phone, and each earlier
  workday's route of a driver's conversation. It hears `routes.work_phones_changed`.
- **`driver_routes`** (Routes): their route today, the day it is in the DSP's timezone, read for
  their IDs of its source, today alone, as live route data last read it while it holds today,
  else as Routes last read it: its code, van, wave, stops and pace, with the work phone they
  hold. Of several, the one with the most stops, as a rescue they drove too has fewer. And for a
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
  fifty latest answered conversations and groups (one listed under a group is the group's, as
  its `groupId` says),
  with the DSP's workspaces (whether it works in each, the address of the app's Messages tab
  there, `chatLink`, whether, on there, it needs updating for what Communications uses,
  `needsUpdate`, and the version of its logo, `logo`, none without one), who is typing, each
  with their person and photo, and the latest change (`revision`).
- `GET …/waiting`: how many conversations wait, and since when each has, for the sidebar; and the
  latest message from whom each is with (`latest`: the conversation, the group it's listed under,
  their name and the message), for alerts.
- `GET …/updates?after=<revision>&typing=<version>`: the long poll, with the workspaces as they
  are.
- `GET …/conversations?q=<name>`, or `?before=<lastAt>:<id>`, with `team=<id>` for one
  workspace's alone, or `kind=number` for the numbers' (`q` a name or a number's digits), or
  `kind=work` with `q` for work conversations: the fifty conversations found closest to a name,
  or the answered ones before the last shown.
- `GET …/conversations/{id}`, `?before=<ts>`: a conversation and its fifty latest messages, or
  those before one, in the order written; by text, `ts` is `<at>:<id>`; with whom it's with's
  conversations of each kind (`elsewhere`), for a driver's, their route `today` and each of its
  days' route and van (`workdays`), and what a message written in it may carry (`limits`: its
  text's characters, photos, each photo's bytes and their kinds; Slack's own, or as `texting`
  says, none by text while nothing offers it).
- `GET …/conversations/{id}/profile`: what the profile shows of whom it's with, a member of a
  workspace or a driver, besides the name, role and photo the conversation has: their route
  `today` (`route`, `van`, `waveAt`, `stops`, `stopsDone`, `pace`, and the work phone they hold,
  `phone` and `phoneSince`), `phone`, `email`, and the workspace's name (`slack`), each none
  when unknown.
- `GET …/routes`: Today's Routes: the live day, its drivers by name, each with their route
  `today` and work conversation, and the work phones nobody held that texted today.
- `POST …/conversations/{id}/driver` (`driverId`): Pick driver, for a work phone nobody held: the
  driver's conversation.
- `POST …/conversations` (`teamId`, `userId`): opens a direct message with a member.
- `POST …/conversations/{id}/messages` (`text`, `photos`): sends an answer, its text and photos, a
  Slack one's added by `…/uploads`, a text's pictures by `…/texts/uploads`, as `limits` says; by
  text, refused as `communications_texting_off` while the DSP doesn't text. Answers the message and the conversation
  it's in now (`conversation`): by text, another once the number changed hands as it went, which
  the page then opens. The member's access is checked again before it answers: once it ended as
  the answer went, what went is kept, and the answer refused.
- `POST …/conversations/{id}/messages/{message}/retry`: sends again a text Dispatch sent that
  failed.
- `POST …/conversations/{id}/done`, and `…/typing`, which says the member is writing.
- `POST …/conversations/{id}/contact` (`name`), `…/block` and `…/unblock`: a number's
  conversation's, as the number's.
- `POST …/texts/conversations` (`number`, or `driverId` for one of today's drivers): opens the
  conversation a text to the number written now is filed in, as filing decides (a driver's while
  they hold it as a work phone), or the driver's, to text them first.
- `POST …/texts/uploads?name=&type=`: a picture for a text not sent yet, as its bytes, kept by
  `texting`.
- `GET …/texts/contacts?q=<name or digits>`: whom the DSP can text, and the numbers blocked.
- `GET …/texts/files/{id}`: a text's picture or file, as `texting` keeps it.
- `GET …/contacts?q=<name>&team=<id>`: whom the DSP can message, every one or those found
  by a name, in every workspace or one, each with their conversation, if any, since when it
  waits (`waitingSince`) and when it was last written in (`lastAt`).
- `GET …/groups/choices`: whom a group message can go to, each person once with their accounts
  (`people`), and the groups to choose from, each holding someone (`today`, `roles`,
  `workspaces`), by their people's keys.
- `POST …/groups` (`name`, `people`, each `teamId` and `userId`, `text`, `photos`): messages a
  group, sent in the background.
- `GET …/groups/{id}`, `?before=<at>:<id>`: a group and its fifty latest messages and answers, or
  those before one, each of the DSP's with how far it has gone (`progress`).
- `GET …/groups/{id}/people`: who it went to, who first sent it and when, and where it stands
  with each.
- `POST …/groups/{id}/messages` (`text`, `photos`), to the same people; `POST …/groups/{id}/done`,
  for each conversation listed under it that waits.
- `POST …/uploads?name=&type=`: a photo for a message not sent yet, as its bytes.
- `GET …/files/{id}`: a photo or file attached to a message.
- `GET …/workspaces/{team}/logo?v=<logo>`: a workspace's logo, the copy Slack keeps of it, read
  through Slack's `kit`; the browser keeps it until its version changes.

Public, on the admin's address: `GET /communications/initials/1/{letters}.png`.

## Data and upkeep

- **Data:** in each DSP's own database, `communications_conversations` (with the group each is
  listed under), `communications_messages` (with the group each went to or answers),
  `communications_files` (each photo and file, and downloading it; a group message's own photos
  too), `communications_changes` (each change to a conversation or a group, the latest thousand,
  for `updates`); `communications_groups` (each group, its name, who made it and whether it
  waits), `communications_group_people` (whom each went to, by workspace and member ID, under
  their name then), `communications_group_messages` (each message to a group, its sender's name
  and the letters of their initials), `communications_deliveries` (each going to each person:
  waiting, sent with their message, or not sent and why) and `communications_group_uploads` (a
  group message's photos as uploaded to each workspace); the files' bytes in the DSP's data,
  `data/communications/files/`, which backups keep. By text: `communications_text_conversations`
  (each a driver's, a work phone's nobody held, or a number's), `communications_texts` (each
  text filed, by Phones' ID, its pictures as Phones keeps them, and the Done that covered it),
  `communications_texting` (the last of Phones' changes filed), `communications_contacts` (the
  names saved for numbers, by whom and when), `communications_blocked` (the numbers blocked, by
  whom and when) and `communications_picked` (the driver picked for a work phone on a day, by
  whom and when). Kept
  until the DSP is deleted, as the rest.
- **Upkeep:** catching up hourly and as the server starts; downloading what's due, sending
  what's due to go to groups' people, filing texts not filed yet, and filing today's again where
  they belong now, every minute; letting go of
  photos added and never sent hourly.
- **Fixture mode** never calls Slack: Slack's fixture answers what Slack itself calls, and
  `integrations/messages/fixture.rs` what only Communications does. Each member's direct
  message with the app is `D` and the rest of their ID, and holds what the app posted there, as
  Slack lists it; Jordan Ellis's holds what they wrote while Dispatch wasn't listening, for
  catching up: a message, a photo, a question with a reply in its thread, the app's own answer,
  and one from before 2026.
- **Server environment:** none of its own.
