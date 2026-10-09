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
  it found), `POST …/new` (a folder, Doc, Sheet or Slides), `POST …/upload` (a file, up to
  100 MB), `GET …/items/{id}/download` and `…/items/{id}/thumbnail`, and `POST …/items/{id}/rename` and `…/trash`. Behind `documents.use` as well, for a member's own Google account:
  `POST /api/dsp/documents/link` and `…/link/finish`. Behind `documents.manage`, the team's
  access: `GET /api/dsp/documents/team`, `POST …/team/email` and `…/team/remove`; and
  `POST /api/dsp/documents/add`, for files picked in Google Drive. Public, the picker's
  window: `GET /api/documents/google/picker`.
- **Browsing:** with `drive.file`, Dispatch reaches only the files it made or was given, so it
  lists them all and keeps the ones inside the DSP's main folder: another DSP's folder made
  with the same account is never listed, found or changed. A file opens in Google, in a tab
  of its own. Google names the connected account for every change Dispatch makes, so Dispatch
  records who added each file and who last changed it, and names them instead.
- **Google sign-in:** the platform's Google client, which this feature reads from the server's
  own settings: `DISPATCH_DEV_GOOGLE_CLIENT_ID` and `…_SECRET` on Dev and its previews
  (`DISPATCH_PRODUCTION_…` on Production), declared as its settings. Without both, no DSP can
  connect Google; with one, the server refuses to start. It returns to
  `/api/documents/google/return`. That route has no session (its cookie stays on Dispatch's own
  site), so it only passes what Google sent on to the DSP's Documents page, which finishes
  the sign-in as the member who started it. Each sign-in is used once, by its starter, within
  ten minutes, and its code is bound to a PKCE verifier only the server holds.
- **Uploads and downloads:** a file streams through Dispatch to Google as it arrives, into
  the folder open, never held whole. Its type is the browser's, but never one of Google's own,
  which Google would convert the bytes into. Like what Dispatch makes, its editors can't share
  it on. A folder uploaded or dropped on the page keeps its folders: Dispatch makes them first.
  A download streams back as Google sends it: an uploaded file as it was, Google's own Docs,
  Sheets and Slides as Word, Excel and PowerPoint files (Google exports those up to 10 MB), so
  someone with no Google account has them too.
- **Pictures:** in the grid, each file's card shows Google's picture of what it holds (a
  Doc's first page, an image, a PDF's cover) once the card nears the screen, and a drawing of
  its kind until then, or when Google made none. Google's link to a picture lasts hours and
  opens only for the account, so Dispatch fetches it with the account's token and hands the
  picture on: members needn't be signed in to Google, and the token never reaches a browser.
  A listing keeps, for its DSP, the links of the files it showed, for an hour; a picture is
  fetched only by a link kept for that DSP, never one a browser sent, only from Google's own
  hosts (a redirect anywhere else is refused), and only as a PNG, JPEG, GIF or WebP of up to
  2 MB; Google refusing one only leaves the drawing. Its address names the picture's version, so each browser keeps a
  picture until the file changes; eight are fetched from Google at once across the server.
- **Adding from Google Drive:** a file someone made directly in Drive is out of `drive.file`'s
  reach until Google's picker gives it. Those who manage Documents pick it in a window of its
  own, signed in to Google as the account that holds Documents: the window has rules of its
  own that let only Google's sign-in and picker run, and the dashboard's stay as they are.
  The window gets its own short-lived token from Google, never the one Dispatch keeps, and
  tells the Documents page what was picked on a channel only Dispatch's own pages reach, with
  a word the page gave it. Dispatch then checks it reaches each file with the account's token,
  so a file picked as another account is refused. One already in Documents becomes visible;
  one elsewhere in the account's Drive moves into the folder open. The picker needs
  `DISPATCH_DEV_GOOGLE_API_KEY` and `…_GOOGLE_APP_ID` (the Google project's number), or
  `DISPATCH_PRODUCTION_…`, beside the sign-in client; without them the menu doesn't offer it,
  and with one alone the server refuses to start.
- **Team access:** everyone who holds `documents.use` gets the main folder shared with them
  as an editor, and through it everything inside: at the Google account they linked, or else
  their Dispatch email. Google gives no notification of its own. Google refuses an address
  that is no Google account, so Dispatch emails that member, once, how to link one (kind
  `documents.google_account`); until then they see the team's files in Dispatch but can't
  edit in Google. Linking signs them in with Google for only `openid email`, keeps the
  address Google proved, and revokes the token at once. Members who leave, or lose
  `documents.use`, lose the share Dispatch gave them. Anyone else the folder is shared with
  in Google Drive is listed for those who manage Documents to remove. Sharing follows the
  team within a minute, and is brought up to date when Google is connected, when a member
  links and when the team panel opens. The main folder and everything Dispatch makes in it
  can't be shared on by its editors (`writersCanShare` off), so the team decides who it's
  shared with. The panel also shows how full the account's storage is.
- **Reconnecting** takes the same Google account only: `drive.file` reaches the files
  Dispatch made through that account and no other.
- **Data:** `documents_connection`, `documents_connect_requests`, `documents_files` and
  `documents_people` (who the folder is shared with, and at what address), in
  each DSP's own database; the account's refresh token, encrypted in the DSP's secrets as `google.enc`.
- **Upkeep:** every minute, the folder is shared anew for each connected DSP whose team
  changed, read from Dispatch alone. Hourly, a connection Google stopped accepting is marked
  broken, and each connected DSP's sharing is checked against Google's own list.
- **Fixture mode** never calls Google: its sign-in comes straight back with
  `fixture:<email>` as the code, and each account's Drive is kept in memory. A member who
  links signs in as `teammate@example.com`, and an address at `example.net` is no Google
  account. Its Drive also holds files made directly in Drive, out of reach until they're
  picked, and in place of Google's picker the page lists them.
- **Page:** Documents, in a DSP's sidebar.
