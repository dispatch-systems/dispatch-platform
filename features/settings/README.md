# Settings

A DSP's Settings page, the DSP's profile there, and the route its onboarding saves the profile
through. Every owner declares its
sections of Settings in its backend manifest, and the page draws what the server says the
member has: core's settings API (`GET /api/dsp/settings`) answers the pages with a section they
see and those sections, a feature's only while the DSP has it and a part's while it has the
part. Mandatory: every DSP has it.

- **Its page** (`#dsp/<id>/settings`) is open to every member: everyone has their own Profile,
  Security and Theme. It draws the open page's name and its sections in their order. A page is
  named in the address (`?tab=<id>`), which links and the browser's Back move; the page opens on
  its first page when the address names one the member lacks, with the code and reads of the
  page it opens on already loading.
- **Its sidebar** takes the app's place while Settings is open (the route's `sidebar`), the
  account menu kept: Back, to the page the member opened Settings from (core keeps it per DSP,
  through moves among Settings' pages), a search, and each page the server lists that has a
  section to draw, under its group (Personal, DSP, Features) with its icon: a shared page's own,
  a feature's page that of its feature's pages. It is the navigation drawer on a narrow screen.
- **Its search** runs over what the server answered for the open DSP: page and section names,
  field and option labels, a section's `keywords`, its `sources` (the collectors its data comes
  from) and values held as text. Every word typed begins a word there, in any case; it finds a
  page once, by the best of what matched (a page's name, a section's, a field's, an option's or
  a keyword, a source, a value), and says what matched. "/" moves to it, the arrow keys through
  what it found, Enter opens one and Escape clears it. Opening one names its section in the
  address (`&find=<section>`): the page scrolls to it and outlines it for a few seconds, and the
  address forgets it. A search starts afresh for another DSP or role.
- **A declared section** is a card: its label, a control for each field by its kind, and Save,
  enabled once a value changed and every value is one the field takes. A save sends the
  changed values with the revision they were read at; one the server says would lose something
  asks first in its words, and one another session beat is refused and the section read again.
  A `schedules` field draws core's schedule editor inline, which asks about unsaved edits before
  the page moves to another of its pages.
- **A custom section** is drawn by the component its owner registers in the `settingsSections`
  slot under the section's id, with `refresh` to read Settings again; with `visible` it says
  whether the member has anything there, and a page with nothing to draw isn't listed.
- **Its section:** `settings.profile`, DSP profile, on the DSP's page (`?tab=dsp`): the DSP's
  name, its short code, which names its address and can't change once given, its station code
  and its timezone. Settings draws it from its declaration (`backend/mod.rs`); it's kept where
  onboarding keeps it, in core's tables, through a `Kept` adapter around core's profile save,
  checked as onboarding checks it. A save is logged as Settings' `settings.changed`, not as
  the DSP's setup; a timezone changed retimes its schedules, and the save wakes the scheduler.
  The DSP's open views expire with it, so the next one opened, the Profile badge's, shows it.
- **Permission:** `settings.manage` (Manage DSP Settings), listed under DSP on the role sheet.
  It gates only the DSP's profile, there and through onboarding, not the page.
- **API:** `POST /api/dsp/profile`, behind `settings.manage`: the DSP's name, abbreviation,
  station code and timezone, as onboarding sets it up, logged as `dsp.profile_completed`. It
  wakes the scheduler, since schedules run in the DSP's time.
- **Setup:** through the `dspSetup` slot, it names that permission and route to core's
  onboarding, which asks an owner of a DSP without its details for them.
- **No storage** of its own: the profile is the DSP's, in core's tables.
