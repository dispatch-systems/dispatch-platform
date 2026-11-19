# Legacy Tracker

A link on Conduct's tab strip to the tracker one DSP kept its writeups in before Conduct. It
opens the address the DSP sets, in a new browser tab, and draws nothing of its own. The address is
the DSP's data, never in this repository. Only an address that begins `https://` is opened: with
none set, or anything else typed, no link is drawn.

- **Switch:** `cjf87gqd_legacy_tracker`, off for every DSP until the platform owner switches it on.
- **Uses:** Conduct (`depends_on`), whose page its link is on.
- **Permissions:** none of its own. Every member may set the address once the feature is on; the link itself is for those holding Conduct's `conduct.use`.
- **API:** `GET /api/dsp/cjf87gqd-legacy-tracker`, behind `conduct.use`: the address the link opens, or none.
- **Tab:** Legacy Tracker, a link after the tabs of Conduct's page (`useLink` in `frontend/feature.ts`).
- **Settings:** `cjf87gqd_legacy_tracker.general`, "Legacy Tracker", on Conduct's page of Settings (`?tab=conduct`) above its categories, for every member (`roles::ACCESS`, `backend/settings.rs`). Its one field, `link`, is the address, empty until the DSP sets one. Core keeps it in the DSP's settings and records each change as `settings.changed`.
- **Data:** none of its own.
