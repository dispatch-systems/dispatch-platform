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

A session belongs to the address it was made at, as its cookie does: a platform owner signs in
at the admin's alone, where every DSP is theirs to open, a member at their DSP's alone, where
that DSP is the only one, and nobody at the invite page. Anywhere else a right password reads
as a wrong one, and a session reads as signed out. A password reset's link goes back to the
address it was asked at, and a passkey belongs to the address it was made at. An invitation's
link opens at its DSP's address, the one its email names, and at the invite page, which it
names while the DSP has no short code, and nowhere else. There a DSP's first owner sets the DSP
up, short code included, as they accept, unless the platform owner already gave it one, and is
then sent to Sign In at the DSP's address.
