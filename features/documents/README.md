# Documents

A DSP's shared folders, Docs and Sheets, kept in a Google account the DSP connects, best one
kept for Documents alone. Dispatch makes one main folder there and reaches the account's whole
Drive (`drive`), so everything in the folder shows, whoever made it and however. Google calls
that scope restricted: unreviewed, up to 100 accounts may grant it over the app's life, each
seeing that Google hasn't verified the app. Past that, no other account can connect until the
platform owner submits the app to Google for review, which takes a yearly security assessment
and weeks, so it's best submitted well before the limit.

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
  `POST /api/dsp/documents/connect`, `…/connect/finish` and `…/disconnect`. A DSP deleted has
  Google take back Dispatch's access first, best effort (`deleting.revoke`), as disconnecting
  does, unless another DSP connects the same account or finishes signing in to it meanwhile, as
  Google would end its access too; and
  its files', pictures' and copies' memory goes (`deleting.forgotten`), with none kept of
  what was under way. Behind `documents.use` too:
  `GET /api/dsp/documents/folder` (a folder and what it holds, or with `q`, what a search inside
  it found), `GET …/folder/read` (the same, or that it hasn't changed), `GET …/query` and
  `…/query/describe` (the same, a page at a time), `POST …/new` (a folder, Doc, Sheet or
  Slides), `POST …/upload` (a file, up to 100 MB), `GET …/items/{id}/download` and
  `…/items/{id}/thumbnail`, and `POST …/items/{id}/rename`, `…/move` and `…/trash`. Behind
  `documents.use` as well, for a member's own Google account:
  `POST /api/dsp/documents/link` and `…/link/finish`, and `POST /api/dsp/documents/add`, for
  files and folders from Google Drive. Public, the picker's page:
  `GET /api/documents/google/picker`.
- **Browsing:** Dispatch reads the main folder from Google a few folders at a time, so another
  DSP's folder made with the same account is never listed, found or changed, and follows what
  changes in the account's Drive. A file someone makes in the folder in Drive shows as the
  folder is read next. A connection made before Dispatch asked for the whole Drive
  (`fullAccess` false) reaches only the files Dispatch made or was given, and reads only those,
  until someone reconnects it: Settings' Google Drive card says `Limited Access` and offers to
  reconnect. A file opens in Google, in a tab of its own. Google names the connected account for every change Dispatch makes, so Dispatch records
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
- **Reading a folder again:** the page reads a folder or a search with
  `GET /api/dsp/documents/folder/read`, which takes what `…/folder` takes, under the same
  permission and the same rules, and optionally `since`: the `revision` of the answer the page
  already has, 64 lowercase hex characters, a SHA-256 of that answer and of what was asked. It
  answers `{ revision, folder }`, and a null `folder` says the answer is exactly the one `since`
  names, which the page goes on using. The server does a full read's work either way: it picks
  the folder out of the whole copy, names who added and last changed each file as they're named
  now, and keeps each picture's link as shown, before it hashes the answer; and it checks the
  member's permission and the connection again after any wait, for both answers. So an
  unchanged answer saves the bytes sent and the browser reading them, not the server's work or
  the page's comparing it with what it holds, and a changed one is written out twice, to hash
  and to send. An unchanged answer doesn't make the copy any newer, or mean Google was asked:
  the copy follows Google by its own five seconds and minute. The page keeps what it read under
  `…/folder`'s address, in its session's bounded cache, and gives each answer, an unchanged one
  too, the moment it asked for it, which a change Google confirmed waits on.
- **Reading a page at a time:** `GET /api/dsp/documents/query` reads a folder or a search in
  pages, and `GET …/query/describe` describes files such a read lists. No page of Dispatch calls
  either yet: Documents reads whole folders, as above. `…/query` takes `id`, `q`, `filter`
  (`all`, `folders`, `docs`, `sheets`, `slides` or `uploads`), `page`, from zero, and `size` (1
  to 100, 100 unless it says), and to go on from an earlier answer, `snapshot`, `manifestSince`
  and `since`. A read without `snapshot` opens: the server builds a compact manifest of
  everything the folder or search holds, unfiltered, and each filter's order over it, in core's
  bounded read cache. That takes memory in proportion to the files, and nothing of it is stored:
  a restart or a restore loses it, as it does the copy. A read with `snapshot` continues: it
  only looks its index up, never builds one, and answers `reset` (`missing` or `changed`) when
  the index is missing, expired or evicted, or no longer current. A snapshot holds for one
  connection generation, one run of the server, one copy and one state of that copy. A `full`
  answer describes at most 100 files; `unchanged` says the page is exactly the one `since` names
  under the manifest `manifestSince` names, and nothing wider. A `full` answer with a null
  manifest keeps the manifest `manifestSince` names: it never means the folder is empty. Who
  added and last changed each file, as named now, and each picture's link as shown go into every
  page before it's hashed, an unchanged one too. `…/query/describe` describes up to 100 files of
  a snapshot by ID, none twice and each one the snapshot holds, in the order asked, under that
  snapshot and the member's access as it is now. Whatever calls these keeps a describe address
  to 4 KiB, or less where its host takes less, drops what it gathered when one answers `reset`,
  and after repeated resets reads the whole folder once instead.
- **Google sign-in:** the platform's Google client, which this feature reads from the server's
  environment: `DISPATCH_DEV_GOOGLE_CLIENT_ID` and `…_SECRET` on Dev and its previews
  (`DISPATCH_PRODUCTION_…` on Production), declared in its `env`. Without both, no DSP can
  connect Google; with one, the server refuses to start. It returns to
  `/api/documents/google/return`. That route has no session (its cookie stays on Dispatch's own
  site), so it only passes what Google sent on to the page the sign-in started on, which
  finishes it as the member who started it: Settings' Connections for the DSP's account and for
  a member linking theirs there, and Documents for a member linking theirs from there, at the
  address it started at: the admin's, or the DSP's own, where its members' sessions are. Its
  state says which. Once the DSP's account is connected, those who hold `documents.use` go on to
  Documents, which starts empty: Dispatch makes the main folder and nothing in it. Each sign-in
  is used once, by its starter, within ten minutes, and its code is bound to a PKCE verifier
  only the server holds.
- **Uploads and downloads:** files picked or dropped on their own are named first, in a
  dialog that shows each name without its extension and keeps the extension; a folder's files
  keep their names. A file streams through Dispatch to Google as it arrives, into the folder
  open, or the folder or step of the path it was dropped on, never held whole. Its type is the browser's, but never one of Google's own,
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
- **Adding from Google Drive:** every member adds a file or folder from elsewhere, from the
  Upload menu beside uploading files and folders. Drive link asks for a Google Drive, Docs,
  Sheets or Slides link, as pasting one on the page (outside a field) does, and adds what it
  names at once when the account that holds Documents reaches it: anything in its Drive or
  shared with it. Else, and from Google Drive in the menu, Google's picker opens on a page of
  its own in the same tab, signed in to Google as the member (`picker.me`: the address the
  folder is shared with them at, or their own): on the files the link named, or on their Drive,
  beside what's shared with them and their shared drives. The page has rules of its own that
  let only Google's picker run, and the dashboard's stay as they are. Documents leaves it what
  to pick, the folder open, and where to go back to in the tab's session storage. The page
  sends the browser to Google's sign-in, which comes straight back when the browser is signed
  in already, to the same return address as connecting, with its answer in the fragment, which
  no server sees. That address takes it on to the picker's page at the address it started at,
  the admin's or the DSP's own, as the state names. The page gets its own short-lived token
  from Google, never the one Dispatch keeps, and names its address to Google, though not the
  page, since the picker's key works only from Dispatch's own addresses. With that token it
  brings each file picked where the account that holds Documents reaches it, as the member:
  one of theirs moves into the folder open, where the team has it; one someone else owns, or
  one it can't move, is shared with that account, as an editor or else to view. Then it goes
  back to Documents, which adds what was picked. Leaving Documents while files upload asks
  first. Dispatch checks the account that holds Documents reaches each file, and refuses one it
  doesn't (`documents_picked_elsewhere`). One already in Documents is recorded as added; one
  elsewhere in the account's Drive moves into the folder open. Only its owner moves a file, so one someone else
  owns and shared with the account, or one Google won't move (`cannotAddParent`), gets a
  shortcut in the folder open instead, as Drive's own Add shortcut does, once however often
  it's added. Documents keeps the file a shortcut stands for beside its tree, and shows the
  shortcut as that file: its kind, its owner's name for it, and when and by whom it last
  changed, which it sorts by. It opens that file, and a download takes that file's bytes. Its
  name is the owner's, so it isn't renamed in Dispatch; trashing it trashes only the shortcut.
  Once the account no longer reaches the file, because its owner trashed it or stopped sharing
  it, the shortcut is `unavailable`: shown faded, opening nothing, and only trashed. A shortcut
  to a folder opens the folder in Drive, since Dispatch can't reach what it holds. The team
  gets the file shared with them, as **Team access** says; one the account may not share on,
  as when it may only view it, can't be, and adding it says the team asks its owner for it
  (`unshared` in the answer). Files in a shared drive come in as shortcuts too: every call
  about one file says Dispatch understands shared drives (`supportsAllDrives`), and listing
  and the change feed include their files. A folder brings only itself:
  what it holds is picked once it's there, and stays where it is. Picking the main folder
  changes nothing. The picker needs
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
  who it's shared with. A file someone else owns that Documents holds a shortcut to is shared
  with them too, as an editor, at the same address, as far as its owner lets the account
  share it: when the shortcut comes in, and whenever the folder's sharing is brought up to
  date. Dispatch keeps the shares it made of such files (`documents_shared_files`) and takes
  back only those: from members who leave or lose `documents.use`, and from everyone once the
  shortcut leaves Documents. A share its owner made stays.
- **Renaming** a file keeps its extension, the letters and digits after its last dot (up to
  eight): the dialog shows the name without it, and `…/rename` refuses a name that changes it
  (`documents_extension_kept`). Folders and Google's own Docs, Sheets and Slides have none, so
  their whole name changes. Each item names its `extension` for the dialog.
- **Moving** a file or folder takes whatever it holds with it, into another folder of
  Documents or the top: dragged onto a folder or a step of the path, or with Move to… in its
  menu, which needs no pointer. A click with Shift, Ctrl or ⌘ selects instead of opening, and
  what's selected drags and moves together. Like trashing, `…/move` asks Google where the file
  is first; it refuses a folder going inside itself (`documents_move_inside`), and does nothing
  to one already there. Only its owner moves a file, so one the account doesn't own, as one a
  member made in Drive, stays (`documents_cannot_move`). Its answer names the folder it was
  in, so Undo, on the toast that says where it went, takes each back.
- **Reconnecting** takes the same Google account only: the folder and its files are that
  account's.
- **A connection replaced meanwhile:** every connection has a generation, an opaque value new
  each time the DSP connects or reconnects (`grantGeneration`: a name for the connection, never
  a token of Google's). The account's access token, the copy and the picture links shown are
  each kept for one environment's storage, one DSP and one generation. Whatever waits on Google
  goes on with the connection it began with, and before each request to Google and before
  keeping anything it checks that the member who asked may still do it, that the DSP is active
  with Documents on, and that the connection is still that one, unbroken. A request Google
  already has may finish, but what it brings is never kept for the connection that replaced it.
  Reconnecting, disconnecting and deleting a DSP forget only what's kept for that environment
  and that DSP. `…/new`, `…/upload`, `…/add` and `…/items/{id}/rename`, `…/move` and `…/trash`
  take `grantGeneration` and refuse (`documents_not_connected`) unless it names the connection
  as it is now; without it, each goes on with the connection it finds. The overview, the
  account, every folder and page read and the picker's setup name the generation, and the page
  sends with each change the one it held when the member began it, never whichever is latest as
  it's sent.
- **Files for other features:** Documents exposes `files` (its contract, in `contract/`), somewhere for a
  feature bringing files in, as Slack does, to keep them. It says why it can't yet, in words a member
  reads: Google Drive isn't connected, or needs reconnecting. It browses the folders, from the main folder down, and finds a
  folder by its ID, the main folder or one inside it;
  makes a folder at the top of the main folder or inside a folder there, as a member; and streams a file of up to
  100 MB into a folder inside the main folder only, never elsewhere in the Drive
  (`files_folder_not_found`). A file saved so is Documents' like any upload, added by the
  member it came from, or by the name of someone who isn't one, as a Slack member no member
  matches. The activity log records each as `<file> from <where>` (`documents.saved`), with that
  member as its actor, or none. It writes a page of HTML, its pictures in it, as a Google Doc in
  such a folder, recorded and logged as a saved file is, as Slack keeps a canvas; and writes a
  page over a Doc it wrote, wherever it's been moved inside the main folder, renamed as the page
  is and changed by whoever changed the page last, without logging it again. A Doc no longer
  inside the main folder isn't written over, and a page over 50 MB, the most Google turns into a
  Doc, isn't written (`upload_too_large`). Those who see its folders hold `documents.use`.
- **Data:** `documents_connection`, `documents_connect_requests`, `documents_files` (with the
  name of whoever added or last changed a file who isn't a member, its own migration 3),
  `documents_people` (who the folder is shared with, and at what address) and
  `documents_shared_files` (the shares Dispatch made of files someone else owns), in each
  DSP's own database; the account's refresh token, encrypted in the DSP's secrets as
  `google.enc`. The connection's row holds its generation (`grant_generation`, migration 4), an
  opaque value that outlasts a restart and is new with every connection saved, one a server
  from before migration 4 saves too. Saving a connection writes its row as broken under the
  new generation, then the encrypted token, then marks the row connected: a save cut short
  leaves the connection broken, never the earlier one still connected over the new one's
  token. A server from before migration 4 writes the token first, so a save of its own cut
  short isn't held to that order.
- **Upkeep:** every minute, the folder is shared anew for each connected DSP whose team
  changed, read from Dispatch alone. Hourly, a connection Google stopped accepting is marked
  broken, and each connected DSP's sharing is checked against Google's own list.
- **Fixture mode** never calls Google: its sign-in comes straight back with
  `fixture:<email>` as the code, and each account's Drive is kept in memory. A member who
  links signs in as `teammate@example.com`, and an address at `example.net` is no Google
  account. A main folder it makes starts empty; reconnected to the one it kept, its team has
  made a folder, a Doc in it and a PDF directly in it meanwhile, which Documents shows without
  their being added. Its Drive also holds a picture elsewhere and two Sheets someone else owns
  and shared with the account, `Route tracker` and `Fuel prices`, to view only, which come in
  once they're picked, and in place of Google's picker the page lists them, with those a pasted
  link names chosen.
- **Page:** Documents, in a DSP's sidebar. It keeps the folders it opened, shows them at once
  when they open again and asks for them anew behind them, every 15 seconds while one is
  open; it asks for a folder as the pointer rests on it. A folder opening for the first time,
  or a search, leaves what was shown until it comes. Making, renaming, moving and trashing show
  at once, and go back with the reason if Google refuses; an upload shows its file as it lands.
