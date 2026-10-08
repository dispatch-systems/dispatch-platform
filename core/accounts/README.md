# Accounts

Users, sessions, passwords, passkeys, authenticator apps and recovery codes; invitations, DSP
onboarding and member profiles; their emails and their screens. Both Settings pages show its
Profile, Security and Theme panels, through `frontend/settings/tabs.ts`.

It serves the public pages an invitation links to, reading and accepting it. A feature invites
through `Store::invite`, as Team does: the platform owner always may, and a member only holding
the one permission a feature marks `.invites()`; without one, only the platform owner invites.
A DSP's onboarding asks for its details only when a feature fills the `dspSetup` slot, naming
who may save them and the route that does, as Settings does.
`frontend/README.md` holds the signed-out screens' rules.
