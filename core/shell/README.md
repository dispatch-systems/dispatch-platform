# Shell

The frontend's frame, which every owner's screens are drawn in. `ui/` knows nothing about the
product, `lib/` imports only itself and core's API types, and features build on `ui/`, `lib/`
and `runtime/` alone, as `app/tests/rules/dashboard-structure.test.ts` checks.

| Folder     | Holds                                                                                                                                       |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `shell/`   | The frame itself: the sidebar, the account menu, the page that draws the open route, and what runs on every DSP page (`Watchers.tsx`).      |
| `ui/`      | The UI kit: tables, tabs, dialogs, fields and the pieces every page is made of, exported from `ui/index.ts`; a widget's, `ui/Widget.tsx`.   |
| `lib/`     | Pure logic: dates and formats, CSV, the response cache, backoff, the performance policy.                                                    |
| `runtime/` | What a page runs on: the API client (`api.ts`), the data cache and prefetch, the session, navigation, permissions, feedback, and the slots. |
| `styles/`  | The tokens every stylesheet uses and the reset.                                                                                             |

## The slots

`runtime/slots.ts` gathers what every owner's `frontend/feature.ts` declares, from one module
per slot under `runtime/slots/`: `pages`, `settings`, `connections`, `integrations`,
`page-tabs`, `account`, `api`, `collected`, `platform`, `watchers` and `widgets`, each declaring what an
owner puts in the slot,
adding its field to `FrontendFeature` by declaration merging, and exporting what a host reads
it with. `installed.ts` holds the manifests the app installs before the first render, and the
three helpers every slot reads them with: `present`, what the owners put in a slot that the
view has; `presentBy`, the same with each owner; and `first`, the first answer any owner gives.
What a feature adds to another's page is there only while the DSP has the feature: switched off
or hidden, it is gone, as are its pages. `core/manifest/README.md` lists every slot.

## Widgets

A widget (the `widgets` slot, `runtime/slots/widgets.ts`) is drawn from `ui/Widget.tsx`, which
its own code imports and loads with it, so each looks and fits its size as the others do:

- `WidgetLink`: the whole widget, a link to what it shows on its owner's page filling its cell,
  under its header: its icon, its label (`short` in one column) and the `day` it shows. With no
  `href`, it opens nothing.
- `WidgetNote`: what it says in place of what it shows, as why it can't or that nothing is in yet.
- `WidgetCount`, a count over what it counts, and `WidgetZero`, a day with none, as good news.
- `WidgetList` of `WidgetRow`s: as many rows as fit, each whole, each with its title, a badge,
  a line under it, initials, a tone at its edge, and at its end a `WidgetValue`, `WidgetPill` or
  `WidgetBar`.
- `WidgetFooter`, its line at its foot from two rows.

`widgetFit({ columns, rows })` names the steps every widget keeps: `narrow` (one column) shows a
count, with names under it once `tall` (two rows), which also has room for the footer; a row's
second line and initials come in `detail` (three columns by two), and more at its end once
`wide` (four columns). `sizesUpTo(most, first)` declares every size to `most`, added at `first`.
A widget's own CSS, under its `className`, draws only what the kit doesn't. Away from Production, Home
shows one widget at every size it declares with `?sizes=<id>`, and `dispatchdev shot --widget
<id>` saves each grid of them. A widget's browser test checks them all with `expectEverySize(page,
dspId, id)` from the test fixtures, once its data is in: on every grid, in a desktop's window and a
laptop's (1440×900), it fails naming each size where the widget doesn't load, shows nothing under
its header, cuts its header's label, or runs past its frame. Home's own test checks every widget
on the seeded DSP. In a single cell, the count's number takes the cell's height.

A manifest is loaded up front, and outside a browser by the route table's tests. It stays
small, imports no CSS, touches no browser API until it is called, and loads its pages lazily.
