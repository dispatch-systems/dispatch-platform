# Team & Roles

A DSP's members, roles and invitations: inviting through core's `Store::invite`, with Invite
Members marked `.invites()`, while core serves the public pages an invitation links to.
Mandatory: every DSP has it.
Nobody grants or manages beyond their own permissions, the Owner role holds every permission
and can't change, and a DSP keeps at least one owner.

- **Permissions:** `members.invite` (Invite Members), `members.manage` (Manage Members) and
  `roles.manage` (Manage Roles), listed under Team on the role sheet. Inviting and managing
  members ask their holder to have verified who they are recently. Any of the three reads the
  member and role lists.
- **API:** `GET /api/dsp/members` (each member, their role, and whether they're online now) and
  `GET /api/dsp/roles`, behind any of the three. Behind `members.invite`:
  `POST /api/dsp/members/invite`, `GET /api/dsp/invitations` (the newest 100) and
  `POST /api/dsp/invitations/revoke`. Behind `members.manage`, `POST /api/dsp/members/{id}`,
  which changes a member's role, or with a null role removes them. Behind `roles.manage`:
  `POST /api/dsp/roles` (a new role), `POST …/{id}` and `POST …/{id}/remove`.
- **Invitations** last seven days. Core sends the email, and an invitation stays good only
  while whoever sent it may still invite, and to that role. The first owner of a DSP still
  being set up is also asked to set it up.
- **Roles:** every DSP starts with Owner, Manager and Member; the Owner role is fixed and holds
  every permission. A DSP has up to 50 roles, each named in 40 characters or fewer and never
  "Owner". A role in use can't be removed until its members move. Someone may give or change
  only roles within their own permissions, and the DSP's last owner can't be moved off Owner
  or removed.
- **Log:** core records every member and role event: `member.invited`, `member.joined`,
  `member.role_changed`, `member.removed`, `invitation.revoked`, and `role.created`,
  `role.updated` and `role.deleted`, which Team's routes cause.
- **Errors** only its screens raise are worded in its API client, so they load with them.
- **Storage:** none of its own. Members, roles and invitations are core's, in the platform's
  database.
