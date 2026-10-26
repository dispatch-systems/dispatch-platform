# Shell

The frontend's frame, which every owner's screens are drawn in. `ui/` knows nothing about the
product, `lib/` imports only itself and core's API types, and features build on `ui/`, `lib/`
and `runtime/` alone, as `app/tests/rules/dashboard-structure.test.ts` checks.

| Folder     | Holds                                                                                                                                       |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `shell/`   | The frame itself: the sidebar, the account menu, the page that draws the open route, and what runs on every DSP page (`Watchers.tsx`).      |
| `ui/`      | The UI kit: tables, tabs, dialogs, fields and the pieces every page is made of, exported from `ui/index.ts`.                                |
| `lib/`     | Pure logic: dates and formats, CSV, the response cache, backoff, the performance policy.                                                    |
| `runtime/` | What a page runs on: the API client (`api.ts`), the data cache and prefetch, the session, navigation, permissions, feedback, and the slots. |
| `styles/`  | The tokens every stylesheet uses and the reset.                                                                                             |

## The slots

`runtime/slots.ts` gathers what every owner's `frontend/feature.ts` declares, from one module
per slot under `runtime/slots/`: `pages`, `settings`, `connections`, `integrations`,
`page-tabs`, `account`, `api`, `collected`, `platform` and `watchers`, each declaring what an
owner puts in the slot,
adding its field to `FrontendFeature` by declaration merging, and exporting what a host reads
it with. `installed.ts` holds the manifests the app installs before the first render, and the
three helpers every slot reads them with: `present`, what the owners put in a slot that the
view has; `presentBy`, the same with each owner; and `first`, the first answer any owner gives.
What a feature adds to another's page is there only while the DSP has the feature: switched off
or hidden, it is gone, as are its pages. `core/manifest/README.md` lists every slot.

A manifest is loaded up front, and outside a browser by the route table's tests. It stays
small, imports no CSS, touches no browser API until it is called, and loads its pages lazily.
