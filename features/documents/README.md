# Documents

A DSP's shared folders, Docs and Sheets, kept in a Google account the DSP connects: a Google
Workspace account or anyone's own Google account. Dispatch makes one main folder there and
works only with what it makes or is given (`drive.file`), so Google asks for no review.

- **Switch:** `documents`, off for every DSP until the platform owner switches it on.
- **Permissions:** `documents.use`, which every default role holds, and `documents.manage`,
  owners only, to connect and disconnect Google.
- **API:** `GET /api/dsp/documents` behind `documents.use`; `POST /api/dsp/documents/connect`,
  `…/connect/finish` and `…/disconnect` behind `documents.manage`.
- **Google sign-in:** the platform's Google client, `DISPATCH_DEV_GOOGLE_CLIENT_ID` and
  `…_SECRET` on Dev (`DISPATCH_PRODUCTION_…` on Production), returning to
  `/api/documents/google/return`. That route has no session (its cookie stays on Dispatch's own
  site), so it only passes what Google sent on to the DSP's Documents page, which finishes
  the sign-in as the member who started it. Each sign-in is used once, by its starter, within
  ten minutes, and its code is bound to a PKCE verifier only the server holds.
- **Reconnecting** takes the same Google account only: `drive.file` reaches the files
  Dispatch made through that account and no other.
- **Data:** `documents_connection` and `documents_connect_requests`, in each DSP's own
  database; the account's refresh token, encrypted in the DSP's secrets as `google.enc`.
- **Upkeep:** hourly, a connection Google stopped accepting is marked broken.
- **Fixture mode** never calls Google: its sign-in comes straight back with
  `fixture:<email>` as the code.
- **Page:** Documents, in a DSP's sidebar.
