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

Every DSP keeps its own people in its own database, its directory: their accounts, roles and
memberships, sign-ins, two-factor and passkeys, password resets and invitations, in tables that
match the platform's. The platform's directory keeps its owners' alone. So one address may hold
an account in several DSPs, each with its own password, and changing one never touches another.
`Store::directory` opens the one an account lives in, and an `Auth`'s `scope` names its DSP.
An invitation's token says nothing of its DSP, so the platform keeps which DSP each is for in
`invitation_routes`. A DSP set up before its people were its own has them copied in once, at
startup (`directories.rs`), where `dispatch-backend directories` reports, read-only, what that
copies; the platform's copy stays for the release before.

A session belongs to the address it was made at, as its cookie does, and is kept in that
address's directory: a platform owner signs in at the admin's alone, where every DSP is theirs
to open, one of a DSP's people at its address alone, where that DSP is the only one, and nobody
at the invite page. Anywhere else a right password reads as a wrong one, and a session reads as
signed out. Failed passwords count against the account in its own directory. A password reset's link goes back to the
address it was asked at, and a passkey belongs to the address it was made at. An invitation's
link opens at its DSP's address, the one its email names, and at the invite page, which it
names while the DSP has no short code, and nowhere else. There a DSP's first owner sets the DSP
up, short code included, as they accept, unless the platform owner already gave it one, and is
then sent to Sign In at the DSP's address.
