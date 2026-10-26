# Phones

A DSP's dispatch phones: Android phones running Dispatch's app, each paired to one DSP, relaying
the texts and pictures its number receives and sends to Dispatch, and sending Dispatch's texts
from that number. No service stands between: a phone opens its own connection to its DSP's
address. Other features read the texts and send them through `texting`; Phones shows none
itself.

- **Switch:** `phones`, off for every DSP until the platform owner switches it on. It needs no
  connection of another's.
- **Permissions:** `phones.manage` (Manage Phones), for its card: pairing, renaming and removing
  the DSP's phones, as core's `connections.manage` (Manage DSP Connections) allows too. Owners hold
  it; another role holds it once the DSP's owner turns it on. A feature reading texts through
  `texting` does so under its own permission.

## The card

On Settings' DSP Connections, for those who manage the DSP's phones or all its connections
(`connectionPieces`, `dsp`): each phone, by its number, with its name, model and the app's version, Online once it
checked in in the last two minutes, else Offline and for how long, read again every 30 seconds;
Rename and Remove in its menu; a name changed and not saved asks "Save changes?", with only
Save and Discard, before the dialog closes; closing the browser tab drops it silently. The card's routes are refused (`not_found`) while Phones is off, though managing
all the DSP's connections still lets a member in. With none, No phone. **Add phone** shows the code that pairs the
next phone, as a QR code its app reads and as its eight letters, and how long it lasts; one
expired gives way to a new one with New code; the dialog closes once a new phone appears, read
every three seconds while it's open. A code needs the DSP's own address, for the phone to
reach: without one, `phones_address_required`.

## Pairing

A code is eight letters of 32 (no 0, O, 1 or I), made by someone who manages the DSP's phones, working once, for ten minutes, one at a time: a new one replaces any not used yet.
Only its SHA-256 is kept. The phone reads it with the DSP's address from the QR code
(`dispatch-phone:pair?site=<address>&code=<code>`) and trades it, with its own number, model and
the app's version, for a key: 32 random bytes in hexadecimal, of which only the SHA-256 is kept.
The first phone is named Dispatch, the next Phone 2 and so on, until renamed. A phone removed
keeps its texts; its key works no more, and its next call is refused as `phones_removed`.

## The phones' address

At the DSP's own address alone, never from a browser (a request with a cookie is refused), while
the DSP is active and Phones is on there (`phones_off` otherwise). `POST /api/phones/pair` is
public, by the code; every other call carries `Authorization: Bearer <key>`, which the route
checks itself, as Slack's signed events are checked (public to the server, which no session
reaches), and counts as checking in.

- `GET /api/phones/outbox`: waits up to 20 seconds for the texts Dispatch has for the phone,
  each to send from its number to `number`, with its pictures to fetch first; a text tried again
  comes again with a later `attempt`. Woken as a text is queued (`phones` topic), and as the
  phone is removed.
- `POST /api/phones/texts`: up to 100 texts the phone reports, received or sent from its own
  messaging app (`clientId`, `direction` in or out, `number`, `body`, `at`, up to ten `files` of
  `name`, `mime` and `size`), each kept once by its `clientId`: answered with Dispatch's ID of
  each and of its files, whose bytes come next.
- `POST /api/phones/files/{id}`: a reported file's bytes, up to 20 MB.
- `GET /api/phones/files/{id}`: a picture of a text Dispatch has for the phone.
- `POST /api/phones/statuses`: how up to 100 attempts at Dispatch's texts went, each by its
  text's `id` and the `attempt` the outbox gave, `sent`, `delivered` or `failed` with its
  `error`. A status only moves on: waiting to sent to delivered, waiting or sent to failed, and
  to delivered from any other, as a delivery report proves it got there. Sent or failed counts
  only for the latest attempt, so a late report of an earlier one never cancels a text tried
  again.

## Texting, what it exposes

`texting` (`dispatch-phones-contract`), while it's on for the DSP: the DSP's phones, those
removed left out, each with whether it's online; a feed of each text as it is now, changed after
a point, oldest change first, with its files, a page at a time, each saying where to read on and
whether more remain;
sending a text from one of the DSP's phones (the one named, else the one that last texted that
number, else the one seen last), with pictures its sender added for it, refused as
`phones_none`, `phones_not_found` or `phones_file_not_found`; trying a failed one again, as a new
attempt, while its phone is still paired; each of the two only while the caller's condition
holds, checked in the write that queues the text, so nothing it reads changes before it's
queued; adding a picture for a text not sent yet, its adder's alone until it's sent, and letting
go of it, as when its adder may no longer send it; and a text's picture or file to answer a page
with. Numbers are kept one way: `+1` and ten digits in North America, `+` and the digits with a
country code, a short code's digits alone; the contract's `normal` writes
any number so, for a feature to match its own to texts'. Its IDs, of phones, texts and files, are
opaque, at most 64 letters, digits, `_` or `-`, as its `identifier` checks one. It publishes `TextsChanged`
(`phones.texts_changed`) once texts or how they went are kept.

## Data and upkeep

- **Data:** in each DSP's own database, `phones_devices` (each phone, by its key's SHA-256),
  `phones_pairings` (the code that pairs the next, by its SHA-256), `phones_texts` (each text,
  with its phone, direction, origin, the other party's number, how one Dispatch sent went, and
  its latest change), `phones_files` (each text's pictures and files, and those added for one
  not sent yet) and `phones_change` (the last change given); the files' bytes in the DSP's data,
  `data/phones/files/`, by their SHA-256, which backups keep. Bytes arrive beside them, and take
  their place, and are let go, only under the platform's write lock with the files that keep
  them. Kept until the DSP is deleted.
- **Upkeep:** hourly, letting go of pictures added and never sent within a day, and of codes
  expired.
- **Demo:** a development or preview DSP has a stand-in phone, `(206) 555-0100`, which no app
  holds, with three texts it received; a text Dispatch sends from it is delivered at once.
- **Log:** `phones.paired`, `phones.renamed` and `phones.removed`, under Connections.
- **Server environment:** none of its own.
