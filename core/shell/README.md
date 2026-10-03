# Shell

The frontend's frame: the sidebar and account menu (`shell/`), the UI kit (`ui/`), pure logic
(`lib/`), the runtime (API client, cache, prefetch, session, navigation, permissions and the
slots each owner's `frontend/feature.ts` fills) and the styles. `ui/` knows nothing about the
product and `lib/` imports only itself and the shared contracts, as
`app/tests/rules/dashboard-structure.test.ts` checks.
