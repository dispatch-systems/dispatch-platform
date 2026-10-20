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
  It hears of each message written to the app (`message.im`), and needs, beside what Slack asks
  for itself (`im:history`, `chat:write`, `chat:write.customize`, `im:write`, `users:read`,
  `users:read.email`), `files:read` to download what members attach, `files:write` to upload the
  photos someone at the DSP adds, and `im:read` to list the app's direct messages to catch up on.

## Messages in

- **Kept:** each message a person writes to the app anew, a file they share there, or a reply
  they write in one of its threads, with its text, when it was written, Slack's IDs of it, its
  workspace and its writer, and its photos and files. Never the app's own posts, another bot's,
  or Slackbot's. Slack's markup is kept as it reads: a link by what it shows, with its address
  when that's another, a mention by the member's name, a channel by its name.
- **Conversations:** one with each member of each workspace, in the direct message the app has
  with them there, under the name their Slack profile last gave them; who they are, their photo
  and their role come from Driver Match (`directory`), by their Slack ID.
- **Edits:** a member editing a message in Slack replaces its text, and marks it edited; nothing
  of what it said before is kept, logged or read. Slack changing a message itself, as to show a
  link's preview, isn't an edit.
- **Deletions:** a message deleted in Slack stays.
- **Photos and files:** downloaded from Slack into Dispatch, kept in the DSP's own data
  (`data/communications/files/`, by their bytes' SHA-256), so they outlive Slack. Each is tried
  at once, then, when it fails, after a minute, five, fifteen, an hour, three, six and twelve,
  and given up after that; it waits, uncounted, while the workspace's admin hasn't approved
  reading files or the workspace needs reconnecting. Only files Slack hosts are downloaded, up to
  100 MB; a larger one, or a link to elsewhere, keeps its name, type and size alone.
- **Never before it was on:** a message is kept only if it was sent once Slack, Communications
  and its integration in that workspace were all on, the latest of the three
  (`feature_on_since`, and Slack's switch record or the workspace's connection): one Slack tells
  of late, or catching up finds, sent before then is left. Slack's other answer, while it's off,
  keeps nothing.
- **Catching up:** as the server starts, and hourly, each direct message the app has in each
  workspace where it's on, once its admin approved reading them, is read from Slack
  (`conversations.history`, and `conversations.replies` for the threads in it) from when it was
  last read, or from when its messages may first be kept, never earlier, so what Dispatch didn't
  hear of while it couldn't hear Slack is kept as Slack would have told of it, once. It reads
  again from a week before where it left off, for replies in threads begun before then, and
  edits made since; where it left off moves on only once it read all there was. Its direct
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
  switched off there, needing reconnecting or disconnected, stays listed with its history, and
  says it can't be written in (`canReply`); sending there is refused (`communications_off`).

## Waiting, Responded and Done

A conversation waits (`waiting`) while the member's latest message came after anyone at the DSP
answered it or marked it done, since the first of their messages after that. An answer makes it
`responded`; Done, `done`, answered without a reply; a new message from the member, waiting
again. It follows from its messages in the order they were written, so one caught up later,
written before an answer, leaves it answered.

## Live

The page waits on `updates`, a long poll as the dashboard's other live pages use (a `Topic`),
which answers once a change it hasn't seen is kept, or who is writing changes, or twenty seconds
pass: each conversation changed, as it is now, and how many wait. A conversation changes with any
of its messages, as one edited or a file kept, so a page with it open reads it again. Who at the DSP is writing in
which conversation is kept in memory alone, each for six seconds after their page last said so
(`typing`), or until they send, and shown to everyone else with Communications.

## Finding people

New message lists every active member of the DSP's workspaces where it's on, with their photo,
role and conversation, by name. A name typed, misspelt or not, finds them, and the list's
conversations, the closest first: each word typed is matched to the word of a name it starts, or
shares its first letter and most of its letters with in order; case and accents don't count. The
letters that matched come with each, as runs of the name's UTF-16 code units, for the page to
mark.

## API

Behind `communications.use`:

- `GET /api/dsp/communications`: the conversations waiting, the longest waiting first, then the
  fifty latest answered, with the DSP's workspaces (whether it works in each, and the address of
  the app's Messages tab there, `chatLink`), who is typing and the latest change (`revision`).
- `GET …/waiting`: how many conversations wait, and since when each has, for the sidebar.
- `GET …/updates?after=<revision>&typing=<version>`: the long poll.
- `GET …/conversations?q=<name>`, or `?before=<lastAt>:<id>`: conversations found, or the
  answered ones before the last shown.
- `GET …/conversations/{id}`, `?before=<ts>`: a conversation and its fifty latest messages, or
  those before one, in the order written.
- `POST …/conversations` (`teamId`, `userId`): opens a direct message with a member.
- `POST …/conversations/{id}/messages` (`text`, `photos`): sends an answer.
- `POST …/conversations/{id}/done`, and `…/typing`, which says the member is writing.
- `GET …/contacts?q=<name>`: whom the DSP can message.
- `POST …/uploads?name=&type=`: a photo for a message not sent yet, as its bytes.
- `GET …/files/{id}`: a photo or file attached to a message.

Public, on the admin's address: `GET /communications/initials/1/{letters}.png`.

## Data and upkeep

- **Data:** in each DSP's own database, `communications_conversations`,
  `communications_messages`, `communications_files` (each photo and file, and downloading it)
  and `communications_changes` (each change, the latest thousand, for `updates`); the files'
  bytes in the DSP's data, `data/communications/files/`, which backups keep.
- **Upkeep:** catching up hourly and as the server starts; downloading what's due every minute,
  and letting go of photos added and never sent hourly.
- **Fixture mode** never calls Slack: Slack's fixture answers what Slack itself calls, and
  `integrations/messages/fixture.rs` what only Communications does. Each member's direct
  message with the app is `D` and the rest of their ID; Jordan Ellis's holds what they wrote
  while Dispatch wasn't listening, for catching up: a message, a photo, a question with a reply
  in its thread, the app's own answer, and one from before 2026.
- **Server environment:** none of its own.
