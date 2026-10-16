# Slack

Dispatch's app in a DSP's Slack workspaces. A DSP adds it to as many workspaces as it uses, and
each workspace belongs to that DSP alone: everything Slack sends names its workspace, and that
alone decides the DSP it reaches. The app does nothing in Slack yet but show, on its Home tab,
which DSP the workspace belongs to; features add what it does, on this foundation.

- **Switch:** `slack`, off for every DSP until the platform owner switches it on. Switched off,
  or for a DSP that isn't active, the app stays in its workspaces and does nothing: Dispatch
  ignores what Slack sends for it, but for the app being removed or losing its tokens, and the
  hourly check passes it by. Switched on again, it carries on.
- **Permissions:** its workspaces are among the DSP's own accounts, so adding Dispatch to them
  and taking it out, and making and cancelling the install link, are for those who hold
  `connections.manage` (Manage DSP Connections), which exists while Slack is on, as they are
  for Paycom's, Cortex's and Google Drive's. `slack.manage` (Manage Slack) is for what Dispatch
  does in each workspace, apart from its connection: its settings. Those who hold it alone see
  the workspaces' cards under DSP Connections too, with Settings and nothing else. Like every
  permission, each starts off for every role; the DSP's owner turns it on.
- **Cards:** under DSP Connections on Settings' Connections, each workspace has a card of its
  own, under its name: whether it works, its name in Slack, who connected it and when, and what
  can be done to it. The install link has one while it works, and an install Slack just sent
  the browser back from has one while it finishes. A DSP with none has no Slack card at all.
- **Settings:** each workspace's Settings opens its settings, a page of their own (`slack`,
  with the workspace and the section open in its address, outside the sidebar, Settings lit
  while it's open): the workspace at the top, and its sections down the side, one shown at a
  time. They're Slack's own, Channels and Integrations, and those other features add for each
  workspace through the `connectionSections` slot under the connection `slack`, each drawn for
  the workspace's ID. A section that names an `integration` is listed under Integrations, and
  only while that's on in the workspace. A feature switched off for the DSP takes its sections
  with it.
- **Integrations:** what other features do through Slack, each declared once in its own feature:
  in its backend manifest's `integrations` (core's `Integration`, with `connection: "slack"`,
  the permissions it `needs`, the events it `hears` and what it does with each, `heard`), and in
  its frontend's `connectionIntegrations`, under the same id, which says for members who don't
  know what it does, how it's used, what Dispatch sees, and what switching it on or off means.
  The feature depends on Slack; Slack names none of them. Integrations, shown while a feature
  the DSP has declares one, has a card for each with its switch, which asks before it switches;
  each starts off in every workspace. The app asks for what every integration needs beside its
  own, whether it's on or not, and Slack hands each event an integration hears of to it once
  Slack's own handling is done, while it's on in the workspace and its feature is on for the
  DSP. Switched off, it hears of nothing, and is never told of what it missed. The activity log
  records each switch, as `<integration> in <workspace>`. A workspace's go with it.
- **Channels:** the workspace's public channels and the private ones someone invited Dispatch to
  in Slack, by name, those Dispatch joined apart from the rest; Slack shows the bot only the
  channels it's in, so being in one is all the access Dispatch has there. Join has Dispatch join
  a public channel, and Leave, leave any channel; a private one it left goes from the list until
  someone invites it back. Up to 2,000 channels are read, 200 at a time, each time the
  section opens. The activity log records each, as `#<channel> in <workspace>`.
- **Names:** a workspace's name in Dispatch is the one it was added with, if it was given one,
  else its name in Slack, which follows Slack's. The pencil by its name at the top of its
  settings changes it, for those who manage the DSP's connections, and blank, has it go by its
  name in Slack again. Up to 60 characters.
- **Connecting:** New on DSP Connections lists Slack, which asks for an optional name and opens
  Connect Slack and Copy install link. Connect Slack sends the browser to Slack, which asks which workspace and for
  the app's permissions, and sends it back to Settings' Connections at the address it started
  at: the admin's, or the DSP's own. The cards finish the install as the member who started it,
  in the session that started it, and why one didn't finish shows at the top of the page. Each
  install is used once, by its starter, within ten minutes. A workspace's Reconnect, when Slack
  stopped honouring its tokens, and Update, when the app asks for permissions its admin hasn't
  approved yet, name the workspace to Slack, which then doesn't ask which. Whoever connected a
  workspace last shows as having connected it, and a workspace the DSP had keeps its name.
  Adding a workspace the DSP has already is refused (`slack_workspace_connected`), as Slack
  can't be asked to leave it out of its choice: Slack installed the app there again, so
  Dispatch keeps the new tokens, and the activity log records it reconnected. A workspace another DSP has is refused (`slack_workspace_taken`), and
  stays that DSP's: Dispatch keeps nothing of the install and never takes the app out of it,
  though Slack, having installed it again, may stop honouring that DSP's tokens, which the
  hourly check then finds. An install Dispatch refuses for any
  other reason after Slack made it, as when the link was cancelled meanwhile, is taken back
  out, so the app is never left in a workspace no DSP holds. One refused while reconnecting a
  workspace the DSP already holds stays installed with the tokens Dispatch had, which the
  hourly check finds if Slack, having installed it again, stopped honouring them. Installing
  across a whole Enterprise Grid organization is refused, and the app taken back out.
- **The install link:** for whoever runs the DSP's Slack, who needn't be in Dispatch. Copy
  install link makes one, in place of any before, with the name given for the workspace it
  adds, and puts it on the clipboard, or shows it there once for a browser that keeps it from
  the clipboard; Dispatch keeps only its hash. It
  works once, for seven days, until it's cancelled, and only while the DSP is active with Slack
  on. Whoever opens it goes on to Slack and, once they allow it, to Dispatch in Slack, in the
  workspace they chose. An install the link started finishes as Slack sends the browser back,
  with no session: the link is all it needs. The workspace's card then shows it connected by
  install link. A link that no longer works, a
  workspace another DSP has, or an install cancelled at Slack ends on a page of its own, which
  says so in a line.
- **Members:** for Driver Match, each workspace's members are read whole once it's connected,
  or its admin approves reading them (`users:read`, `users:read.email`), and hourly; and one at
  a time as Slack tells of someone joining (`team_join`) or changing their profile
  (`user_change`). Each keeps their name, the name they go by in Slack when it's another, the
  email address and phone number on their profile, whether their account is active and
  whether they're a guest. Bots and Slackbot aren't kept; guests are, and a member Slack
  deactivates stays, inactive. Up to 10,000 are read, 200 at a time; a list longer than that
  is refused whole (`slack_members_too_many`). When any member is new, changed or gone, Driver
  Match tells people apart at once. A workspace whose admin hasn't approved reading them is
  passed by, and keeps working otherwise.
- **People:** Driver Match reads the members as its `slack` kind of data, by Slack's own IDs,
  `<workspace>:<member>`: one person for each email address, whichever of the DSP's workspaces
  they're in, under the member ID of the workspace connected first, by their name and the one
  they go by in Slack, listed while their account is active. Slack lists whom the DSP can
  reach, not whom it employs (`contacts_only`): it never keeps someone from having left, and
  someone only Slack knows works in the office while their account is active.
- **Disconnecting** a workspace forgets it, its members, its integrations and its tokens, frees it for any DSP, and then has
  Slack remove the app from it. The activity log records connecting, reconnecting, renaming,
  disconnecting, making and cancelling the install link, and, with no member as their actor,
  a workspace connected by the link, the app removed from a workspace in Slack, and Slack
  refusing a workspace's tokens.
- **What Slack sends:** events to `POST /slack/events`, and what people do with the app's
  buttons and forms to `POST /slack/interactions`, both on the admin's address. Each request
  must carry Slack's signature of its time and body, made with the app's signing secret, and be
  no more than five minutes old: anything else is refused (`slack_unsigned`, 401). Dispatch
  answers at once, as Slack waits three seconds, and does the work after; an event Slack sends
  again is handled once. The app's removal from a workspace, or its tokens' revocation there, is
  kept before Dispatch answers, and a failure is answered as one, so Slack sends it again. A
  request may be up to 64 KiB, as any request to the server may. The app is told when its Home
  tab is opened, which shows the DSP's name; when someone joins a workspace or changes their
  profile, which its members keep; when it's removed from a workspace, which Dispatch
  then forgets, with its members; and when its tokens there are revoked, which breaks the workspace until it's
  reconnected. The app has no buttons or forms yet, so interactions are only answered for now:
  a feature's buttons, forms and shortcuts are handled there, for their workspace's DSP, as
  events are.
- **Calling Slack:** everything that acts in Slack holds a `Bot`, the app's bot in one DSP's
  workspace, whose `call` calls any of Slack's API methods there. `Bot::serving` finds the bot
  for a workspace Slack named, while Dispatch acts there. The app's tokens rotate: each lasts
  twelve hours, and the bot renews one before using it once less than an hour and five minutes
  is left, so the hourly check renews it before it ends, with a refresh token Slack honours
  once. One renewal runs at a time for each workspace, and the new pair is written at once. A
  token Slack says expired is renewed and the call made again, once, unless another call has
  renewed it meanwhile. When Slack asks Dispatch to slow down, it waits as long as Slack
  says, up to half a minute, and tries again, twice at most. Every call is logged by its method
  and how long Slack took (`slack.api`), and each refusal with Slack's reason (`slack.refused`),
  never what was sent.
- **Adding to what the app does:** another feature adds what it does through Slack as an
  integration (above). For Slack itself, `backend/app.rs` is the app as Slack knows it. A
  capability that needs a permission adds it to `SCOPES`, and each workspace then shows Update
  until its admin approves it, as for an integration's; one that listens for an event adds it to
  `EVENTS`. Its handler is a module
  of its own, like `home.rs`, reached from its line in `inbound.rs`'s routing tables:
  `on_event` for events, `on_interaction` for buttons, forms and shortcuts. Then
  `dispatch-backend slack-manifest` prints the app to paste into Slack. The app asks for
  `team:read`, to keep each workspace's name as Slack has it, and for the channels' settings
  `channels:read` and `groups:read`, to list them, `channels:join`, to join public ones, and
  `channels:manage` and `groups:write`, to leave them; and for Driver Match `users:read` and
  `users:read.email`, to read its members with their email addresses.
- **API:** behind `connections.manage`: `GET /api/dsp/slack/account`, the DSP's workspaces and
  install link; `POST /api/dsp/slack/connect` (with `team`, to reconnect or update one),
  `…/connect/finish`, `…/workspaces/{team}/rename`, `…/workspaces/{team}/disconnect`, `…/link`
  and `…/link/cancel`; `…/connect` and `…/link` take the workspace's `label`. The account is
  read behind `slack.manage` too, with the integrations on in each workspace. Behind
  `slack.manage`: `GET …/workspaces/{team}/channels`, and
  `POST …/workspaces/{team}/channels/{channel}/join` and `…/leave`, each answering with the
  channel; and `POST …/workspaces/{team}/integrations/{integration}`, with `on`, answering with
  the account. Public:
  `GET /api/slack/return`, where Slack sends the browser back; `GET /api/slack/install`, the install
  link; and Slack's own `POST /slack/events` and `POST /slack/interactions`.
- **Data:** `slack_workspaces` (each with its name in Dispatch, `label`), `slack_members` (its
  own migration 3), `slack_integrations` (migration 4: those on in each workspace, with who
  switched each on and when),
  `slack_connect_requests` (each with the workspace it named to Slack, to reconnect or update,
  and the name the workspace it adds is to have) and `slack_install_links`, in each DSP's own
  database; `slack_teams`, which DSP each workspace belongs to, in the platform's;
  each workspace's tokens, encrypted in the DSP's secrets as `slack-<team>.enc`.
- **Upkeep:** hourly, each working workspace of each active DSP with Slack on is asked for, as
  the bot: that renews its token as it nears its end, keeps the workspace's name as Slack has
  it, reads its members, and marks it broken if Slack no longer honours its tokens, so its card offers to
  reconnect it.
- **Fixture mode** never calls Slack: installing comes straight back with `fixture:<team>` as
  the code, the first of its workspaces (Operations, Drivers, Managers) that no DSP has yet,
  or the one being reconnected. Its tokens name their workspace; one whose ID holds `GONE` is
  refused. Each workspace has the same channels, `general`, `dispatch`, `photos`, `random` and
  the private `managers`, the bot in `general` and `managers` to start, and what it joins and
  leaves is kept for the server's run. Its members are some of the demo's people, by
  example.com addresses: Jordan Ellis in Operations and Drivers, the owner Morgan Lane in
  Operations and Managers, and in Drivers Cameron Hayes, Casey Rivera by phone, the guest Drew
  Sullivan and the deactivated Jesse Kline, with Slackbot and a bot everywhere. It signs with `fixture-signing-secret`, so tests send
  requests as Slack would.

## Slack's app

The platform's Slack app, one for each environment: **Dispatch Dev** for Dev and its previews,
**Dispatch** for Production, so the two are never mistaken for each other in a workspace. Each
is distributed to any workspace from Slack's Manage Distribution, without a listing in Slack's
Marketplace.

The server reads its settings, a group it needs whole: `DISPATCH_DEV_SLACK_CLIENT_ID`,
`…_SLACK_CLIENT_SECRET` and `…_SLACK_SIGNING_SECRET` on Dev and its previews, or
`DISPATCH_PRODUCTION_…` on Production. Without them, Slack in New says it's unavailable; with
only some, the server refuses to start.

`dispatch-backend slack-manifest` prints the server's app as Slack's manifest: its name, its
Home tab, the permissions it asks for, the events it's told of, the addresses Slack reaches it
at, and its tokens rotating. To make the app, or change it, paste that into Slack:

1. At [api.slack.com/apps](https://api.slack.com/apps), Create New App, From a manifest, in the
   platform's own workspace; or, for an app made already, App Manifest in its settings.
2. Under Basic Information, copy its Client ID, Client Secret and Signing Secret into the
   server's settings above, and restart it.
3. Under Event Subscriptions, have Slack check the Request URL again, now that the server
   answers it.
4. Under Manage Distribution, Activate Public Distribution, so any workspace can install it.
5. Under Basic Information, give it its icon: `tooling/icons/dispatch-dev.png` for Dispatch
   Dev, with its DEV band, or `tooling/icons/dispatch.png` for Dispatch, each drawn from the SVG
   beside it.
