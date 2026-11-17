# HR Line

One DSP's own line of phones for HR, apart from Dispatch Phone's: an Android phone running DSP
Dispatch: Relay, as a dispatch phone does, whose texts are HR Line's alone. Its phones serve its
line of Dispatch Phone's phones ([Lines](../../../communications/README.md#dispatch-phones)): they
relay through Dispatch Phone's phones' address, by the key each was given, and nothing of theirs
is ever in Dispatch Phone's conversations, counts, banners or alerts, nor does Dispatch Phone
answer from them; nor does HR Line see or change Dispatch Phone's own.

- **Switch:** `cjf87gqd_hr_line`, its DSP's alone (`custom`), off until the platform owner switches it
  on. Its code depends on Dispatch Phone (`depends_on: ["communications"]`), and it works only
  while Dispatch Phone is on for the DSP: its routes are refused (`not_found`) while it's off.
- **Permissions:** `cjf87gqd_hr_line.phones` (Manage HR Line Phones), for its card: pairing, renaming
  and removing its phones, as core's `connections.manage` (Manage DSP Connections) allows too.
  Owners hold it; another role holds it once the DSP's owner turns it on.
- **Its card:** HR Line, on Settings' DSP Connections beside Dispatch Phone's, for those who manage
  its phones or all the DSP's connections: Dispatch Phone's card (`PhonesCard`) for its own phones
  alone, with the same rows, Add phone, Download app, Rename and Remove. A code it makes pairs a
  phone to HR Line and replaces only HR Line's code not used yet; its first phone is named HR
  Line, the next HR Line 2, and so on.
- **Routes:** `/api/dsp/cjf87gqd-hr-line/phones`, `…/phones/pairing`, `…/phones/pairing/{id}`,
  `…/phones/{id}/rename` and `…/phones/{id}/remove`, behind `connections.manage` or
  `cjf87gqd_hr_line.phones`: Dispatch Phone's for a line (`dispatch_communications::lines`), for
  `LINE` alone. A phone or code of Dispatch Phone's own is not found here, as HR Line's is on
  Dispatch Phone's.
- **Log:** `cjf87gqd_hr_line.phones.paired`, `cjf87gqd_hr_line.phones.renamed` and
  `cjf87gqd_hr_line.phones.removed`, under Connections, each by the phone's name and number.
- **Stores:** nothing of its own yet: its phones and their texts are kept by Dispatch Phone's
  phone side, by their line.
