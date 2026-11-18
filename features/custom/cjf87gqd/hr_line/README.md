# HR Line

One DSP's own line of phones for HR, apart from Dispatch Phone's: an Android phone running DSP
Dispatch: Relay, as a dispatch phone does, whose texts are HR Line's alone, read and answered on
HR Line's own page. Its phones serve its line of Dispatch Phone's phones
([Lines](../../../communications/README.md#dispatch-phones)): they relay through Dispatch Phone's
phones' address, by the key each was given, and nothing of theirs is ever in Dispatch Phone's
conversations, counts, banners, alerts or pushes, nor does Dispatch Phone answer from them; nor
does HR Line see or change Dispatch Phone's own. Someone who texts both has a conversation on each.

- **Switch:** `cjf87gqd_hr_line`, its DSP's alone (`custom`), off until the platform owner switches it
  on. Its code depends on Dispatch Phone (`depends_on: ["communications"]`), and it works only
  while Dispatch Phone is on for the DSP: its routes are refused (`not_found`) while it's off.
  Switched off, its phones keep relaying and its texts are kept.
- **Permissions:** `cjf87gqd_hr_line.use` (Use HR Line), for its page: reading its conversations and
  answering them, its contacts, and being told of its new texts. `cjf87gqd_hr_line.phones` (Manage
  HR Line Phones), for its card: pairing, renaming and removing its phones, as core's
  `connections.manage` (Manage DSP Connections) allows too. Owners hold both; another role holds
  each once the DSP's owner turns it on.
- **Its page:** HR Line, in the DSP's sidebar for those who use it, with how many of its
  conversations wait beside it: Dispatch Phone's page (`lines.ts`) for its own line alone, headed
  HR Line. Its list is Recents, of its numbers' conversations, those waiting first, searched by
  name or number, and its Contacts, A to Z, with the numbers it blocked and Unblock; a
  conversation opens with its texts, its pictures and files, and their videos; answering it
  sends from HR Line's phones alone, with pictures added; Done, Next waiting, Retry, who else is
  writing; and a number's Save contact (a name, HR Line's own), Copy number, and Block or Unblock,
  on HR Line alone. New message texts a number first. Each is named as Driver Match knows the
  number's person, else by the name HR Line saved, else by the number. Without Slack, Today's
  Routes, groups, a profile, conversations elsewhere, quick replies or GIFs, which are Dispatch
  Phone's own. Its phones offline show in its heading.
- **Display:** `cjf87gqd_hr_line.display` (HR Line), each member's own, on Settings' Display page
  beside Dispatch Phone's Message display, with the same choices but the profile's, and its own
  preview by text: how its page looks to them, and how a new text alerts them. With both on the
  page, each is headed with its line's name.
- **On every page:** `cjf87gqd_hr_line.alerts`, alerting those who use it of its new texts, by its
  own Display section's Sound and Notifications, each titled "HR Line · " and who wrote, opening
  its conversation on HR Line's page, and of its phones as each goes down; and
  `cjf87gqd_hr_line.phones`, a banner atop every page while one of its phones is down (HR Line
  phone offline for 25 min, or by its name while it has several), linking to its card for those
  who manage it. Each is Dispatch Phone's (`MessageAlerts`, `PhonesDown`) for HR Line's line.
- **Pushed:** with Dispatch closed, or on a phone, each new text goes to each browser kept for HR
  Line, of a member who may use it with its Notifications on, titled "HR Line · " and who wrote,
  opening HR Line's page (`PUSHED`, through `lines::pushing`, every minute from
  its DSP's upkeep, `dsp_maintenance`, as after a restart). A browser is kept for HR Line apart
  from Dispatch Phone, and let go of for one alone.
- **Its card:** HR Line, on Settings' DSP Connections beside Dispatch Phone's, for those who manage
  its phones or all the DSP's connections: Dispatch Phone's card (`PhonesCard`) for its own phones
  alone, with the same rows, Add phone, Download app, Rename and Remove. A code it makes pairs a
  phone to HR Line and replaces only HR Line's code not used yet; its first phone is named HR
  Line, the next HR Line 2, and so on.
- **Routes:** behind `cjf87gqd_hr_line.use`, Dispatch Phone's for a line
  (`dispatch_communications::lines`), for `LINE` alone, under `/api/dsp/cjf87gqd-hr-line`: the list
  (`…`), `…/waiting`, `…/updates`, `…/conversations` (search and older), `…/conversations/{id}`,
  `…/window`, `…/messages`, `…/messages/{message}/retry`, `…/done`, `…/typing`, `…/contact`,
  `…/block` and `…/unblock`; `…/texts/conversations` (texting a number first),
  `…/texts/contacts`, `…/texts/uploads`, `…/texts/files/{id}`, `…/preview` and `…/fetch`; and
  `…/push` and `…/push/remove`. Behind `connections.manage` or `cjf87gqd_hr_line.phones`:
  `…/phones`, `…/phones/pairing`, `…/phones/pairing/{id}`, `…/phones/{id}/rename` and
  `…/phones/{id}/remove`. A conversation, text, phone or code of Dispatch Phone's own is not
  found here, as HR Line's is on Dispatch Phone's.
- **Log:** `cjf87gqd_hr_line.phones.paired`, `cjf87gqd_hr_line.phones.renamed` and
  `cjf87gqd_hr_line.phones.removed`, under Connections, each by the phone's name and number; and
  `cjf87gqd_hr_line.contact_saved`, `cjf87gqd_hr_line.number_blocked` and
  `cjf87gqd_hr_line.number_unblocked`, under Settings, as Dispatch Phone's own are, by who it's
  with and the number, never what a text says.
- **Texting:** off (`LINE.texting`): its phones' texts are kept, not filed, until the next
  release turns it on ([Filed once a line texts](../../../communications/README.md#texts)); till
  then its page has no conversations.
- **Stores:** nothing of its own: its phones, texts, saved names, blocks and the browsers told of
  it are kept by Dispatch Phone, by their line; each member's Display choices by Settings.
