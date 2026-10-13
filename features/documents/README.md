# Documents

A DSP's shared folders, Docs and Sheets, kept in a Google account the DSP connects: a Google
Workspace account or anyone's own Google account. Dispatch makes one main folder there and
works only with what it makes or is given (`drive.file`), so Google asks for no review.

- **Switch:** `documents`, off for every DSP until the platform owner switches it on.
- **Permissions:** `documents.use`, to see and work in the DSP's Documents: everyone who holds
  it works in it alike. Like every permission, it starts off for every role; the DSP's owner
  turns it on. Documents once had `documents.manage` as well, which included it; it's retired
  into `documents.use`, so a role kept from before uses Documents.
- **Connections:** the DSP's Google account is one of its own accounts, so Documents declares
  `dsp_connection` and its card sits under DSP Connections on Settings' Connections, beside
  Paycom's and Cortex's: connecting, reconnecting, disconnecting, and how full the account is,
  for those who hold `connections.manage` (Manage DSP Connections), which exists while
  Documents is on. Each member's own Google account has its card under Personal Connections,
  for everyone who holds `documents.use` (but the platform owner, who isn't one of the DSP's
  members). The Documents page sends those who manage the connection there.
- **API:** `GET /api/dsp/documents` behind `documents.use`. Behind `connections.manage`:
  `GET /api/dsp/documents/account`, the account with its storage, and
  `POST /api/dsp/documents/connect`, `…/connect/finish` and `…/disconnect`. Behind `documents.use` too:
  `GET /api/dsp/documents/folder` (a folder and what it holds, or with `q`, what a search inside
  it found), `POST …/new` (a folder, Doc, Sheet or Slides), `POST …/upload` (a file, up to
  100 MB), `GET …/items/{id}/download` and `…/items/{id}/thumbnail`, and `POST …/items/{id}/rename` and `…/trash`. Behind `documents.use` as well, for a member's own Google account:
  `POST /api/dsp/documents/link` and `…/link/finish`. Behind `connections.manage`,
  `POST /api/dsp/documents/add`, for files and folders picked in Google Drive. Public, the picker's
  page: `GET /api/documents/google/picker`.
- **Browsing:** with `drive.file`, Dispatch reaches only the files it made or was given, so it
  lists them all and keeps the ones inside the DSP's main folder: another DSP's folder made with
  the same account is never listed, found or changed. A file opens in Google, in a tab of its
  own. Google names the connected account for every change Dispatch makes, so Dispatch records
  who added each file and who last changed it, and names them instead.
- **The copy:** the server keeps the list of each DSP's Documents in memory (each file's name,
  type, folder, size, who changed it when and Google's links, never what a file holds), and a
  folder opens and a search finds from it. It keeps only what's inside the main folder, packed:
  each file once, sharing its folder's ID and who changed it. Google's change feed brings it up
  to date: in the background once it's five seconds old, and before it answers once it's a
  minute old. What Dispatch changes goes in as soon as Google confirms it, and a feed that
  describes the file from before doesn't undo it. A copy nobody has looked at for half an hour
  goes, so the server keeps only those of the DSPs using Documents. A server that starts, or a
  DSP that connects again or comes back, lists the Drive whole, as does a feed that lost its
  place or one that brings a folder in from elsewhere in the account, since the copy doesn't
  know what it holds. Trashing asks Google where the file is first, in case someone moved it out
  of Documents in Drive. Every call to Google is logged with how long it took
  (`documents.google`), and each refusal with Google's reason and its words, without the IDs
  and addresses in them (`documents.google_refused`).
- **Google sign-in:** the platform's Google client, which this feature reads from the server's
  own settings: `DISPATCH_DEV_GOOGLE_CLIENT_ID` and `…_SECRET` on Dev and its previews
  (`DISPATCH_PRODUCTION_…` on Production), declared as its settings. Without both, no DSP can
  connect Google; with one, the server refuses to start. It returns to
  `/api/documents/google/return`. That route has no session (its cookie stays on Dispatch's own
  site), so it only passes what Google sent on to the page the sign-in started on, which
  finishes it as the member who started it: Settings' Connections for the DSP's account and for
  a member linking theirs there, and Documents for a member linking theirs from there. Its
  state says which. Once the DSP's account is connected, those who hold `documents.use` go on to
  Documents, which starts empty: Dispatch makes the main folder and nothing in it. Each sign-in
  is used once, by its starter, within ten minutes, and its code is bound to a PKCE verifier
  only the server holds.
- **Uploads and downloads:** files picked or dropped on their own are named first, in a
  dialog that shows each name without its extension and keeps the extension; a folder's files
  keep their names. A file streams through Dispatch to Google as it arrives, into the folder
  open, never held whole. Its type is the browser's, but never one of Google's own,
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
  A listing keeps, for its DSP, the links of the files it showed, for an hour, and asks Google
  for a new link once the one it has is an hour old; a picture is fetched only for a file
  kept for that DSP, never by a link a browser sent, only from Google's own
  hosts (a redirect anywhere else is refused), and only as a PNG, JPEG, GIF or WebP of up to
  2 MB; Google refusing one only leaves the drawing. Its address names the picture's version, so each browser keeps a
  picture until the file changes; eight are fetched from Google at once across the server. The
  page asks for it as large as the screen shows it sharply (`s`): 600 pixels on a high-density
  screen, 300 on an ordinary one.
- **Adding from Google Drive:** a file or folder someone made directly in Drive is out of
  `drive.file`'s reach until Google's picker gives it. Those who manage the DSP's connections,
  who hold its Google account, pick it on a page of its own in the same tab, signed in to
  Google as that account: the page has rules of its own that let only Google's picker run, and
  the dashboard's stay as they are. The Upload menu holds them beside uploading files and
  folders, for those who can: Google Drive opens the picker on the folder open in Documents,
  and Drive link asks for a Google Drive, Docs, Sheets or Slides link and opens it on the files
  the link names, or on the folder, as pasting one on the page (outside a field) does.
  Documents leaves the picker's page what to pick and where to go back to in the tab's session
  storage. The page sends the browser to Google's sign-in, which comes straight back when the
  browser is signed in to that account already, to the same return address as connecting, with
  its answer in the fragment, which no server sees. That address takes it on to the picker's
  page at the address it started at, the admin's or the DSP's own, as the state names. The page
  gets its own short-lived token from Google, never the one Dispatch keeps, and names its
  address to Google, though not the page, since the picker's key works only from Dispatch's own
  addresses. It checks Google signed in the account that holds Documents and offers to switch
  if not, then opens the picker. Once something is picked, or the picker is closed, it goes
  back to Documents, which adds what was picked. Leaving Documents while files upload asks
  first. Dispatch checks it reaches each file with the account's token, so a file picked as
  another account is refused. One already in Documents becomes visible; one elsewhere in the
  account's Drive moves into the folder open. A folder brings only itself: what it holds is
  picked once it's there, and stays where it is. Picking the main folder changes nothing. The
  picker needs
  `DISPATCH_DEV_GOOGLE_API_KEY` and `…_GOOGLE_APP_ID` (the Google project's number), or
  `DISPATCH_PRODUCTION_…`, beside the sign-in client; without them the page doesn't offer it,
  and with one alone the server refuses to start.
- **Team access:** everyone who holds `documents.use` gets the main folder shared with them
  as an editor, and through it everything inside: at the Google account they linked, or else
  their Dispatch email. Google gives no notification of its own. Google refuses an address
  that is no Google account, so Dispatch emails that member, once, how to link one (kind
  `documents.google_account`), and the Documents page asks them to instead of showing the
  team's files: it opens once the folder is shared with them. Until Documents first asks
  Google about a member, their sharing is `pending`, and opening Documents asks now rather
  than within the minute. The gate is the page's: `documents.use` still decides what the API
  allows. Linking signs them in with Google for only `openid email`, keeps the address Google
  proved, and revokes the token at once. Members who leave, or lose `documents.use`, lose the
  share Dispatch gave them. Sharing follows the team within a minute, and is brought up to date
  when Google is connected and when a member links. The main folder and everything Dispatch
  makes in it can't be shared on by its editors (`writersCanShare` off), so the team decides
  who it's shared with.
- **Renaming** a file keeps its extension, the letters and digits after its last dot (up to
  eight): the dialog shows the name without it, and `…/rename` refuses a name that changes it
  (`documents_extension_kept`). Folders and Google's own Docs, Sheets and Slides have none, so
  their whole name changes. Each item names its `extension` for the dialog.
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
  account. Its Drive also holds files and a folder made directly in Drive, out of reach until
  they're picked, and in place of Google's picker the page lists them, with those a pasted
  link names chosen.
- **Page:** Documents, in a DSP's sidebar. It keeps the folders it opened, shows them at once
  when they open again and asks for them anew behind them, every 15 seconds while one is
  open; it asks for a folder as the pointer rests on it. A folder opening for the first time,
  or a search, leaves what was shown until it comes. Making, renaming and trashing show at once,
  and go back with the reason if Google refuses; an upload shows its file as it lands.
