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
showing people, who holds each ID of a source, and `people::changed` has people told apart at
once when a feature's change outside a collection. The sources and kinds are shared words, written
to TypeScript in `api/generated/`. Each kind is
written there with its label, unit and icon too, in their order (`driver-data.ts`, which
imports the icons from lucide-react by the names the kinds declare), and only the lists of
people load it. The features whose data names people are listed apart, by their switches
(`people-features.ts`), small enough for a frontend manifest to ask whether a DSP has one on.
