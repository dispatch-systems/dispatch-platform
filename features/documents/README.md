# Documents

A DSP's shared folders, Docs and Sheets, kept in a Google account the DSP connects: a Google
Workspace account or anyone's own Google account. Dispatch makes one main folder there and
works only with what it makes or is given (`drive.file`), so Google asks for no review.

- **Switch:** `documents`, off for every DSP until the platform owner switches it on.
- **Permissions:** `documents.use`, to see and work in the DSP's Documents, and
  `documents.manage`, which includes it, to connect and disconnect Google. Like every
  permission, both start off for every role; the DSP's owner turns them on.
- **API:** `GET /api/dsp/documents` behind `documents.use`; `POST /api/dsp/documents/connect`,
  `…/connect/finish` and `…/disconnect` behind `documents.manage`. Behind `documents.use` too:
  `GET /api/dsp/documents/folder` (a folder and what it holds, or with `q`, what a search inside
  it found), `POST …/new` (a folder, Doc, Sheet or Slides), and `POST …/items/{id}/rename` and
  `…/trash`.
- **Browsing:** with `drive.file`, Dispatch reaches only the files it made or was given, so it
  lists them all and keeps the ones inside the DSP's main folder: another DSP's folder made
  with the same account is never listed, found or changed. A file opens in Google, in a tab
  of its own. Google names the connected account for every change Dispatch makes, so Dispatch
  records who added each file and who last changed it, and names them instead.
- **Google sign-in:** the platform's Google client, which this feature reads from the server's
  own settings: `DISPATCH_DEV_GOOGLE_CLIENT_ID` and `…_SECRET` on Dev and its previews
  (`DISPATCH_PRODUCTION_…` on Production). Without both, no DSP can connect Google. It returns to
  `/api/documents/google/return`. That route has no session (its cookie stays on Dispatch's own
  site), so it only passes what Google sent on to the DSP's Documents page, which finishes
  the sign-in as the member who started it. Each sign-in is used once, by its starter, within
  ten minutes, and its code is bound to a PKCE verifier only the server holds.
- **Reconnecting** takes the same Google account only: `drive.file` reaches the files
  Dispatch made through that account and no other.
- **Data:** `documents_connection`, `documents_connect_requests` and `documents_files`, in
  each DSP's own database; the account's refresh token, encrypted in the DSP's secrets as `google.enc`.
- **Upkeep:** hourly, a connection Google stopped accepting is marked broken.
- **Fixture mode** never calls Google: its sign-in comes straight back with
  `fixture:<email>` as the code, and each account's Drive is kept in memory.
- **Page:** Documents, in a DSP's sidebar.
