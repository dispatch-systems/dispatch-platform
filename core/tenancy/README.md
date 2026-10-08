# Tenancy

DSPs, roles and permission checks, the feature switches, audit recording and the session's DSP
view. Screens and routes check permissions, never features: a permission exists only while its
feature is on. Switching a feature off removes its pages, permissions and automation for that
DSP and deletes nothing it stored.

A page's parts switch on their own inside it: its tabs, and any other part a feature declares
as a sub-feature, each with the permissions it owns, which exist while it and its page are on.
A permission grants everything it implies, however many steps away; one declared `under`
another grants it too and sits under it on the role sheet. Every permission starts off in
every role: the DSP's owner turns them on, and owners hold them all.
