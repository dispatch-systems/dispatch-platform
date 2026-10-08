# Tenancy

DSPs, roles and permission checks, the feature switches, audit recording and the session's DSP
view. Screens and routes check permissions, never features: a permission exists only while its
feature is on. Every feature is mandatory, which every DSP has, or optional, off for every DSP
until the platform owner switches it on. Switching one off removes its pages, permissions and
automation for that DSP and deletes nothing it stored. One made optional later stays on, once,
for every DSP that had it.

A page's parts switch on their own inside it: its tabs, and any other part a feature declares
as a sub-feature, each with the permissions it owns, which exist while it and its page are on.
Only a mandatory feature's parts may be mandatory. A part may need a connection of its own;
switched off, a page keeps its parts as they were, and they come back with it.

The platform owner can also hide an optional feature or part from a DSP: its members, owners
included, lose its pages, tabs and permissions as if it were off, while everything it runs
keeps running and it stays switched on. A hidden page hides its parts. What every DSP has, and
a connection, are never hidden. A platform owner opens a DSP as the Platform Owner, with its
owners' permissions and the hidden features too, or through any of its roles, which see what
that role's members see. Agent keys and connected apps are the platform owner's own, so they
read a hidden feature as well.
A permission grants everything it implies, however many steps away; one declared `under`
another grants it too and sits under it on the role sheet. Every permission starts off in
every role: the DSP's owner turns them on, and owners hold them all.
