# Manifest

What a feature or a collector declares, and the one registry the app builds from them and
installs at startup. Core reaches features and collectors only through `registry()`, never by
name. The frontend's slots are typed in `core/shell/frontend/runtime/slots.ts`.

It holds the people slot's words too (`people`): the sources whose IDs name drivers, the kinds
of collected data that name them and where a person stands, which Driver Match and the
features that name people share, written to TypeScript in `api/generated/`. Each kind is
written there with its label and unit too, in their order (`driver-data.ts`), and its icon is
in its feature's frontend `people` slot, which the lists of people load.
