# Slack

Dispatch's app in a DSP's Slack workspaces. A DSP adds it to as many workspaces as it uses, and
each workspace belongs to that DSP alone: everything Slack sends names its workspace, and that
alone decides the DSP it reaches. The app does nothing in Slack yet but show, on its Home tab,
which DSP the workspace belongs to; features add what it does, on this foundation.

- **Switch:** `slack`, off for every DSP until the platform owner switches it on. Switched off,
  or for a DSP that isn't active, the app stays in its workspaces and does nothing: Dispatch
  ignores what Slack sends for it, but for the app being removed or losing its tokens, and the
  hourly check passes it by. Switched on again, it carries on.
- **Permissions:** `slack.manage` (Manage Slack), to add Dispatch to the DSP's workspaces and
  take it out, and to make and cancel the install link: apart from managing the DSP's other
  connections. Like every permission, it starts off for every role; the DSP's owner turns it
  on. Its workspaces are among the DSP's own accounts, so its card sits under DSP Connections
  on Settings' Connections, beside Paycom's, Cortex's and Google Drive's, which that section
  shows to those who hold `connections.manage` (Manage DSP Connections) as well: it exists
  while Slack is on.
- **Connecting:** Connect Slack, or Add workspace once there's one, sends the browser to Slack,
  which asks which workspace and for the app's permissions, and sends it back to Settings'
  Connections at the address it started at: the admin's, or the DSP's own. The card finishes
  the install as the member who started it, in the session that started it. Each install is
  used once, by its starter, within ten minutes. A workspace's Reconnect, when Slack stopped
  honouring its tokens, and Update, when the app asks for permissions its admin hasn't approved
  yet, name the workspace to Slack, which then doesn't ask which. Whoever connected a workspace
  last shows as having connected it. A workspace another DSP has is refused
  (`slack_workspace_taken`), and stays that DSP's: Dispatch keeps nothing of the install and
  never takes the app out of it, though Slack, having installed it again, may stop honouring
  that DSP's tokens, which the hourly check then finds. An install Dispatch refuses for any
  other reason after Slack made it, as when the link was cancelled meanwhile, is taken back
  out, so the app is never left in a workspace no DSP holds. One refused while reconnecting a
  workspace the DSP already holds stays installed with the tokens Dispatch had, which the
  hourly check finds if Slack, having installed it again, stopped honouring them. Installing
  across a whole Enterprise Grid organization is refused, and the app taken back out.
- **The install link:** for whoever runs the DSP's Slack, who needn't be in Dispatch. Copy
  install link makes one, in place of any before, puts it on the clipboard and shows it on the
  card; Dispatch keeps only its hash. It works once, for seven days, until it's cancelled, and
  only while the DSP is active with Slack on. Whoever opens it goes on to Slack and, once they
  allow it, to Dispatch in Slack, in the workspace they chose. An install the link started
  finishes as Slack sends the browser back, with no session: the link is all it needs. The
  card then lists the workspace as connected by install link. A link that no longer works, a
  workspace another DSP has, or an install cancelled at Slack ends on a page of its own, which
  says so in a line.
- **Disconnecting** a workspace forgets it and its tokens, frees it for any DSP, and then has
  Slack remove the app from it. The activity log records connecting, reconnecting,
  disconnecting, making and cancelling the install link, and, with no member as their actor,
  a workspace connected by the link, the app removed from a workspace in Slack, and Slack
  refusing a workspace's tokens.
- **What Slack sends:** events to `POST /slack/events`, and what people do with the app's
  buttons and forms to `POST /slack/interactions`, both on the admin's address. Each request
  must carry Slack's signature of its time and body, made with the app's signing secret, and be
  no more than five minutes old: anything else is refused (`slack_unsigned`, 401). Dispatch
  answers at once, as Slack waits three seconds, and does the work after; an event Slack sends
  again is handled once. A request may be up to 64 KiB, as any request to the server may. The
  app is told when its Home tab is opened, which shows the DSP's name and a button to its
  address; when it's removed from a workspace, which Dispatch then forgets; and when its tokens
  there are revoked, which breaks the workspace until it's reconnected. The Home tab's button
  only opens Dispatch, so interactions are only answered for now: a feature's buttons, forms and
  shortcuts are handled there, for their workspace's DSP, as events are.
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
- **Adding to what the app does:** `backend/app.rs` is the app as Slack knows it. A capability
  that needs a permission adds it to `SCOPES`, and each workspace then shows Update until its
  admin approves it; one that listens for an event adds it to `EVENTS`. Its handler is a module
  of its own, like `home.rs`, reached from its line in `inbound.rs`'s routing tables:
  `on_event` for events, `on_interaction` for buttons, forms and shortcuts. Then
  `dispatch-backend slack-manifest` prints the app to paste into Slack. The app asks for
  `team:read` alone today, to keep each workspace's name as Slack has it.
- **API:** behind `slack.manage`: `GET /api/dsp/slack/account`, the DSP's workspaces and
  install link; `POST /api/dsp/slack/connect` (with `team`, to reconnect or update one),
  `…/connect/finish`, `…/workspaces/{team}/disconnect`, `…/link` and `…/link/cancel`. Public:
  `GET /api/slack/return`, where Slack sends the browser back; `GET /api/slack/install`, the install
  link; and Slack's own `POST /slack/events` and `POST /slack/interactions`.
- **Data:** `slack_workspaces`, `slack_connect_requests` and `slack_install_links`, in each
  DSP's own database; `slack_teams`, which DSP each workspace belongs to, in the platform's;
  each workspace's tokens, encrypted in the DSP's secrets as `slack-<team>.enc`.
- **Upkeep:** hourly, each working workspace of each active DSP with Slack on is asked for, as
  the bot: that renews its token as it nears its end, keeps the workspace's name as Slack has
  it, and marks it broken if Slack no longer honours its tokens, so the card offers to
  reconnect it.
- **Fixture mode** never calls Slack: installing comes straight back with `fixture:<team>` as
  the code, the first of its workspaces (Operations, Drivers, Managers) that no DSP has yet,
  or the one being reconnected. Its tokens name their workspace; one whose ID holds `GONE` is
  refused. It signs with `fixture-signing-secret`, so tests send requests as Slack would.

## Slack's app

The platform's Slack app, one for each environment: **Dispatch Dev** for Dev and its previews,
**Dispatch** for Production, so the two are never mistaken for each other in a workspace. Each
is distributed to any workspace from Slack's Manage Distribution, without a listing in Slack's
Marketplace.

The server reads its settings, a group it needs whole: `DISPATCH_DEV_SLACK_CLIENT_ID`,
`…_SLACK_CLIENT_SECRET` and `…_SLACK_SIGNING_SECRET` on Dev and its previews, or
`DISPATCH_PRODUCTION_…` on Production. Without them, the card says Slack is unavailable; with
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
5. Under Basic Information, give it its icon.
