# Manifest

What a feature or a collector declares, and the one registry the app builds from them and
installs at startup. Core reaches features and collectors only through `registry()`, never by
name. The frontend's slots are typed in `core/shell/frontend/runtime/slots.ts`.

It holds the people slot too (`people`). A feature whose data names people declares a `People`
for each kind of data: what it is (`PeopleData`), whose IDs it names them by (`SourceData`) and
who it names, most often read with one query (`people::named`), with what else it knows in
`.records`, `.workdays`, `.driver_departments`, `.photos`, `.initials()` and
`.contacts_only()`. The one feature that
tells people apart reads them all, and answers the directory: `people::who` says, for a feature
showing people, who holds each ID of a source and the version of their photo, `people::photo`
gives that photo, from whichever feature holds one, for the feature to serve behind its own
permission, and `people::changed` has people told apart at once when a feature's change
outside a collection. The dashboard draws a person with `PersonAvatar`, their photo or, without
one, their initials. The sources and kinds are shared words, written
to TypeScript in `api/generated/`. Each kind is
written there with its label, unit and icon too, in their order (`driver-data.ts`, which
imports the icons from lucide-react by the names the kinds declare), and only the lists of
people load it. The features whose data names people are listed apart, by their switches
(`people-features.ts`), small enough for a frontend manifest to ask whether a DSP has one on.

It holds the services slot too (`services`): what a feature offers every other feature through
a contract core defines, so one feature uses another's work without depending on it or knowing
which feature does it. Each service is a trait in a module of its own in `backend/services/`,
named for what it does; a feature offers it in its manifest's `services`, one feature at most
offers each, and a feature using it asks for it for a DSP (`files::for_dsp`), which answers only
while the feature offering it is on there. The first is `files`, somewhere to keep a DSP's
files in folders: its name and the permission that shows its folders, the largest file it
takes, why it can't keep files yet, browsing its folders, a folder by ID, making a folder,
saving a file streamed as it arrives (`Upload::streamed`) with who it came from, a member
or someone known only by name, and writing a page of HTML as a document members open and edit,
then over it as the page changes. The services offered are written to TypeScript too
(`api/generated/services.ts`): `offered(name, view)` in the frontend's runtime answers a page
using one whether the DSP has it, what it's called and the permission that shows what it
keeps, without naming the feature offering it.
