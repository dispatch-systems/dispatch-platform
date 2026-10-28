# The phones' protocol

What Dispatch's Android app and its DSP say to each other: every call the app makes, at the DSP's
own address (`https://<code>.dspdispatch.com`), under `/api/communications/phone`, and what each
answers. The handlers are beside this file, each with its limits at its top;
[Communications' README](../../README.md#dispatch-phones) says what Communications does with what
they carry.

Every call sends and answers JSON (`Content-Type: application/json`), but a file's bytes. Times
are epoch milliseconds, sizes bytes, and lengths characters. A refusal answers its HTTP status
with `{"error": "<code>", "message": "…"}`: the app acts on the code, never the message.

## Who may call

- **Where:** at the DSP's own address alone, while the DSP is active. Anywhere else, or carrying
  a cookie (as a browser would), a call is refused `sign_in_required` (401). While
  Communications is off for the DSP, every call is refused `phones_off` (403).
- **Pairing** takes the code someone at the DSP showed (`POST /api/communications/phone/pair`), and answers the
  phone's key: 64 hexadecimal letters (`KEY_BYTES` in hexadecimal), given once, which the app
  keeps.
- **Every other call** carries `Authorization: Bearer <key>`, once. Without it, or with a key no
  phone holds, it's refused `sign_in_required` (401); once the phone was removed,
  `phones_removed` (401): its key works no more, so the app pairs again with a new code.

## Presence

A phone is online while it checked in within the last two minutes (`ONLINE_MS`). Waiting on the
outbox, reporting texts and reporting statuses check in, and pairing counts as the first; sending
or fetching a file's bytes doesn't. So the app waits on its outbox again as each wait ends, which
keeps it online.

## Limits

- **A call's JSON:** at most 64 KiB, arriving within 15 seconds (core's request body limit),
  else `request_too_large` (413) or `request_timeout` (408). A batch within the counts below can
  still be larger: the app splits a batch by its size too.
- **A batch:** 1 to 100 texts reported, or statuses, in one call; the outbox answers at most 100
  at once (`MOST_TEXTS`).
- **A text** (`LIMITS`, which answering from Dispatch is held to too): a body of at most 10,000
  characters, and at most 10 pictures or files, each at most 20 MiB.
- **A file's bytes:** one upload, `Content-Type: application/octet-stream` with its
  `Content-Length`, at most 20 MiB: refused `upload_too_large` (413) past it, and
  `length_required` (411) without it. It fails `upload_incomplete` (400) when fewer bytes come
  than it said, or none come for a minute, and is refused `uploads_busy` (503) while the server
  takes four others: try it again.
- **A number:** at most 32 characters of digits, spaces and `+-().` (`NUMBER_MOST`). Dispatch
  writes it one way: `+1` and ten digits in North America, `+` and the digits with a country
  code, a short code's digits alone.
- **IDs** Dispatch gives, a text's or a file's: at most 64 letters, digits, `_` or `-`
  (`ID_MOST`), meaning nothing to the app: it sends them back as they came.

## Calls

### `POST /api/communications/phone/pair`

```json
{ "code": "K7Q2-M9XA", "number": "(206) 555-0100", "model": "Pixel 8a", "version": "1.0.0" }
```

- `code`: the eight letters, as the QR code (`dispatch-phone:pair?site=<address>&code=<code>`)
  or someone typed them: case, spaces and dashes don't matter, 8 to 16 characters
  (`TYPED_MOST`).
- `number`: the phone's own, empty when it doesn't know it. `model`: at most 100 characters
  (`MODEL_MOST`). `version`: the app's, at most 40 (`VERSION_MOST`).

Answers `{ "key": "<64 hex>", "phone": { "id", "name" }, "dsp": { "name" } }`: the phone is
named Dispatch, or Phone 2 and so on, until someone renames it. A code works once, for ten
minutes, and only while no newer one replaced it: else `phones_pairing_invalid` (401), and
someone at the DSP makes a new one.

### `GET /api/communications/phone/outbox`

Waits up to 20 seconds (`WAIT`) for the texts Dispatch has for this phone, and answers as soon as
it has any, or with none once the wait ends; it wakes as a text is queued for the phone, and as
the phone is removed.

```json
{
  "texts": [
    {
      "id": "…",
      "number": "+14255550199",
      "body": "On my way",
      "attempt": 41,
      "files": [{ "id": "…", "name": "door.jpg", "mime": "image/jpeg", "size": 48213 }]
    }
  ]
}
```

Each, oldest first, is to send `body` from this phone to `number`, with each file fetched first
(`GET /api/communications/phone/files/{id}`). A text stays in the outbox until a status of its latest
`attempt` says it was sent or failed, so it comes again with each wait: the app keeps the
`(id, attempt)` pairs it has started, and never sends one twice. A text tried again comes with a
later `attempt`: a new one to send. Refused `platform_busy` (503) while the server holds too many
waits: wait a moment and ask again.

### `POST /api/communications/phone/texts`

The texts the phone received, and those sent from its own messaging app:

```json
{
  "texts": [
    {
      "clientId": "sms-8812",
      "direction": "in",
      "number": "+14255550199",
      "body": "Gate code?",
      "at": 1793041515267,
      "files": [{ "name": "gate.jpg", "mime": "image/jpeg", "size": 48213 }]
    }
  ]
}
```

- `clientId`: the app's own ID of the text, 1 to 128 characters (`CLIENT_ID_MOST`), never reused
  on this phone.
- `direction`: `in`, to the phone, or `out`, from it. `number`: the other party's. `at`: when it
  was written. `files`: optional, each `name` at most 200 characters and `mime` at most 100
  (`NAME_MOST`, `MIME_MOST`), with the `size` the phone knows.

Answers, for each, Dispatch's ID of it and of its files, in the order reported:
`{ "texts": [ { "clientId": "sms-8812", "id": "…", "files": ["…"] } ] }`. Reporting is
idempotent: a text reported again by its `clientId` is kept once and answered the same, so the
app reports until it's answered, then sends each file's bytes.

### `POST /api/communications/phone/files/{id}`

The bytes of a file of a text the phone reported, by the ID its report answered. Sent again, they
replace those before. The file's size is what arrives, whatever the report said. Refused
`phones_file_not_found` (404) for a file not of this phone's reported texts.

### `GET /api/communications/phone/files/{id}`

A picture of a text in this phone's outbox, as a download of its name and type. Refused
`phones_file_not_found` (404) for one of no text waiting for it.

### `POST /api/communications/phone/statuses`

How the attempts at the outbox's texts went:

```json
{
  "statuses": [
    { "id": "…", "attempt": 41, "status": "failed", "at": 1793041519000, "error": "No service" }
  ]
}
```

- `id` and `attempt`: as the outbox gave them. `status`: `sent`, `delivered` or `failed`, and
  `at`, when it was. `error`: why it failed, at most 500 characters (`ERROR_MOST`), kept with
  `failed` alone.

Answers `{ "ok": true }`. Reporting is idempotent, and order doesn't matter: a status only moves
on, waiting to sent to delivered, waiting or sent to failed; sent or failed counts only for the
text's latest attempt, so a late report of an earlier one never cancels a text tried again; and
delivered counts from any attempt, as it proves the text got there. A status of a text that
isn't this phone's is ignored.

## Failures

A call that fails without an answer, or with a `5xx`, is safe to make again: reports and statuses
are idempotent, and a file's bytes replace themselves. The app tries again with a growing pause,
and gives up on nothing: a text only fails once its phone says so, or once it's removed, when
every text waiting on it fails with `phones_removed` as its error.

Pairing alone isn't safe to make again. Its code works once, and Dispatch keeps only the SHA-256
of the key it answers, so a pairing whose answer was lost may have paired a phone whose key was
lost with it: tried again, the code is refused `phones_pairing_invalid`. The app then pairs with a
new code, and someone at the DSP removes the phone the lost answer paired, which nothing can use.
