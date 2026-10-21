# Communications

The DSP's direct messages with the members of its Slack workspaces. Employees write to the DSP's
Dispatch app in Slack, in its Messages tab; anyone at the DSP whose role gives them
Communications reads each conversation on the Communications page and answers it there, and the
answer reaches the employee's direct message in Slack under the full name and photo of whoever
wrote it. It works through Slack, on Slack's own foundation for direct messages: while it's on
in a workspace, Slack no longer answers what's written to the app that DSP Messaging is
unavailable.

- **Switch:** `communications`, off for every DSP until the platform owner switches it on. It
  depends on Slack (`depends_on: ["slack"]`): it hears nothing while Slack is off for the DSP.
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

## The page

- **Communications** in the sidebar, for those with `communications.use`, with how many
  conversations wait beside it (`waiting`, read every 30 seconds while the page is closed, and
  as the number changes while it's open). The page fills the window, however wide (`fills`),
  the list beside the conversation open, each scrolling on its own, and everything on it grows
  with the page's own width, smoothly, from a laptop's to an ultrawide's, then stops (a
  container query on the page). On a wide page, the messages and the box under them keep to a
  centred column. Under 760 pixels of page, as on a phone, one pane at a time, edge to edge:
  the list, or the conversation open, which takes the whole page, with a way back. The
  conversation open is in the address (`?open=<id>`).
- **The list:** with two or more workspaces where Communications works, which it shows, every
  one or one of them, each by its logo (else the first letters of its name), as the Update
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
  Enter to send; while writing, teammates see it. Photos show by Driver Match's person for each
  (`/api/dsp/people/{code}/photo`), never Slack's own addresses.
- **The profile** of whom a conversation is with opens from who it's with, above it, or their
  photo by their messages, over the conversation, and closes with its X, Escape, or a press
  anywhere else; from 1,500 pixels of page, it opens beside the conversation instead, a column
  of its own covering nothing, and closes with its X or Escape. Its X and Escape put focus back
  on what opened it. Their photo, name and role; Call, while they have a phone number; Today:
  their route on the DSP's own day, its van, when its wave starts and how many stops it has;
  Contact: their phone number, email address and the workspace the conversation is in. Each is
  left out when nothing knows it, and Today when they have no route today. No feature has a page
  of a person to open, so it has no Full profile.
- **New message** finds whom to write to by name, misspelt or not, and opens their
  conversation, or opens a direct message with them, kept at the top of the list until it holds
  a message. With nothing typed, Message a group comes first.
- **Copy chat link** copies the address that opens the app's Messages tab in a workspace where
  Communications works, chosen from a menu with several, for employees to write to the DSP.
  Neither it nor New message shows without such a workspace.
- **Setting Slack up**, for those who also hold `connections.manage`, which connects and updates
  the DSP's workspaces: while the DSP has none, Connect Slack takes the inbox's place; while a
  workspace where Communications is on hasn't approved all it uses (`needsUpdate`), Update
  Slack covers the page, not the sidebar, with no way to close it, each such workspace with
  Update and Copy link (its own install link, for whoever approves apps in that Slack), until
  an update brings the workspaces with none needing it, within twenty seconds. Connecting or
  updating from here comes back here from Slack, which finishes it. Everyone else sees the page
  as it is, empty without Slack.

## Messaging a group

One message to many members at once, as "the 9:20 wave is running 20 minutes late": each gets it
in their own direct message with the app, and their answers come back together.

- **Whom to:** Message a group, in New message, opens it beside the list, where the message being
  written sits at the top. To finds people as New message does, and groups by name; each pick is
  a chip, taken out again with its X, or Backspace. Below it, the groups to choose from, each
  ticked and a chip once picked, and each left out while it holds nobody: **Today**, everyone on
  a route on the DSP's own day, then each wave by when it starts, as Routes has read the day
  (`driver_routes`), while it's on and has read some; **Roles**, drivers, helpers and the office,
  as Driver Match knows them; and with two or more workspaces where Communications works,
  **Workspaces**, everyone and each workspace's. Today's and the roles' groups hold their people
  in every workspace where Communications works; a workspace picked narrows them to it, and
  picked alone is everyone in it; someone picked by name always gets it. Each person gets it
  once, by one of their accounts: the one picked by name, else the first in a workspace picked,
  the one they last wrote from first. It's called what was picked: one pick's name, else theirs
  joined, someone by their first name, and past three the first two and how many others.
- **Writing it:** as in a conversation, photos too, sent with Send to and how many it goes to.
- **Sending:** in the background, one at a time, at most one a second in each workspace, as Slack
  asks of an app posting messages (`chat:write`), in each person's direct message, opened first
  if need be (`im:write`), under the sender's full name and photo as an answer goes
  (`chat:write.customize`), its photos uploaded once to each workspace (`files:write`). Slack
  asking Dispatch to slow down (429, Retry-After) is waited out as long as it says, up to two
  minutes, and beyond that tried again two minutes later; a try that fails is tried again after a
  minute, five and fifteen, then given up; one to a workspace where Communications no longer works, or someone no longer in it,
  isn't sent at all. Each is kept until it's sent or given up, so sending picks up after a
  restart, from the pass every minute; a try that may have posted before the server stopped
  looks for its post in their direct message first (`im:history`), by the metadata each post
  carries, and keeps it rather than send a second.
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

## What it reads of other features

The profile, and the groups a group message can go to, read what other features expose, each
only while the feature offering it is on for the DSP, for anyone with `communications.use`, who
needs no permission of theirs:

- **`directory`** (Driver Match): who the member is, by their Slack ID, and the first phone
  number and email address their sources have for them (Driver Profiles' roster, Slack's
  profile).
- **`driver_routes`** (Routes): their route today, the day it is in the DSP's timezone, read for
  their IDs of its source, today alone, as Routes last read it: its code, van, wave and stops.
  Of several, the one with the most stops, as a rescue they drove too has fewer. And for a group
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
- `GET …/waiting`: how many conversations wait, and since when each has, for the sidebar.
- `GET …/updates?after=<revision>&typing=<version>`: the long poll, with the workspaces as they
  are.
- `GET …/conversations?q=<name>`, or `?before=<lastAt>:<id>`, with `team=<id>` for one
  workspace's alone: the fifty conversations found closest to a name, or the answered ones
  before the last shown.
- `GET …/conversations/{id}`, `?before=<ts>`: a conversation and its fifty latest messages, or
  those before one, in the order written.
- `GET …/conversations/{id}/profile`: what the profile shows of whom it's with, besides the
  name, role and photo the conversation has: their route `today` (`route`, `van`, `waveAt`,
  `stops`), `phone`, `email`, and the workspace's name (`slack`), each none when unknown.
- `POST …/conversations` (`teamId`, `userId`): opens a direct message with a member.
- `POST …/conversations/{id}/messages` (`text`, `photos`): sends an answer.
- `POST …/conversations/{id}/done`, and `…/typing`, which says the member is writing.
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
  `data/communications/files/`, which backups keep. Kept until the DSP is deleted, as the rest.
- **Upkeep:** catching up hourly and as the server starts; downloading what's due, and sending
  what's due to go to groups' people, every minute; letting go of photos added and never sent
  hourly.
- **Fixture mode** never calls Slack: Slack's fixture answers what Slack itself calls, and
  `integrations/messages/fixture.rs` what only Communications does. Each member's direct
  message with the app is `D` and the rest of their ID, and holds what the app posted there, as
  Slack lists it; Jordan Ellis's holds what they wrote while Dispatch wasn't listening, for
  catching up: a message, a photo, a question with a reply in its thread, the app's own answer,
  and one from before 2026.
- **Server environment:** none of its own.
