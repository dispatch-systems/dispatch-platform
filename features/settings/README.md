# Settings

A DSP's Settings page, and the route that saves the DSP's profile. Every owner declares its
sections of Settings in its backend manifest, and the page draws what the server says the
member has: core's settings API (`GET /api/dsp/settings`) answers the pages with a section they
see and those sections, a feature's only while the DSP has it and a part's while it has the
part. Mandatory: every DSP has it.

- **Its page** (`#dsp/<id>/settings`) is open to every member: everyone has their own Profile,
  Security and Theme. It draws a tab for each page the server lists that has a section to draw,
  in the server's order, and the open page's sections in theirs. A page is named in the address
  (`?tab=<id>`), which Back and links move; the page opens on its first page when the address
  names one the member lacks, with the code and reads of the page it opens on already loading.
- **A declared section** is a card: its label, a control for each field by its kind, and Save,
  enabled once a value changed and every value is one the field takes. A save sends the
  changed values with the revision they were read at; one the server says would lose something
  asks first in its words, and one another session beat is refused and the section read again.
  A `schedules` field draws core's schedule editor inline, which asks about unsaved edits before
  the page moves to another of its pages.
- **A custom section** is drawn by the component its owner registers in the `settingsSections`
  slot under the section's id, with `refresh` to read Settings again; with `visible` it says
  whether the member has anything there, and a page with nothing to draw has no tab.
- **Permission:** `settings.manage` (Manage DSP Settings), listed under DSP on the role sheet.
  It gates only saving the DSP's profile, not the page.
- **API:** `POST /api/dsp/profile`, behind `settings.manage`: the DSP's name, abbreviation,
  station code and timezone. It wakes the scheduler, since schedules run in the DSP's time.
- **Setup:** through the `dspSetup` slot, it names that permission and route to core's
  onboarding, which asks an owner of a DSP without its details for them.
- **No storage** of its own: the profile is the DSP's, in core's tables.
