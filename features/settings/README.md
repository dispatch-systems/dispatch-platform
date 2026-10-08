# Settings

A DSP's Settings page: the account's Profile, Security and Theme panels, the tabs other
owners add through the settings tabs slot and the pieces they add under a tab's own panel
through the settings pieces slot, and the route that saves the DSP's profile. A feature's tabs
and pieces are there only while the DSP has it, and a part's while it has the part.
Mandatory: every DSP has it.

- **Its page** (`#dsp/<id>/settings`) is open to every member: everyone has their own Profile,
  Security and Theme. Each other tab shows to those its owner lets see it, such as Routes'
  Data tab to those who manage routes. A tab is named in the address (`?tab=<id>`), and the
  page opens on it with its code and its pieces' code already loaded.
- **Permission:** `settings.manage` (Manage DSP Settings), listed under DSP on the role sheet.
  It gates only saving the DSP's profile, not the page.
- **API:** `POST /api/dsp/profile`, behind `settings.manage`: the DSP's name, abbreviation,
  station code and timezone. It wakes the scheduler, since schedules run in the DSP's time.
- **Setup:** through the `dspSetup` slot, it names that permission and route to core's
  onboarding, which asks an owner of a DSP without its details for them.
- **No storage** of its own: the profile is the DSP's, in core's tables.
