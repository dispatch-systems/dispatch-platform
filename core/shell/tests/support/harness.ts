import type { Page } from '@playwright/test';
import react from '@vitejs/plugin-react';
import { build, type Rolldown } from 'vite';

// Dashboard code a browser test draws on a page of its own, with no server: a module that renders
// itself into the page's `#root`, built as the app is, held in memory and served from there.

/** Where it's served, an address nothing else answers. */
const ORIGIN = 'http://harness.test';

/**
 * Opens `entry`, a module that renders itself into `#root`, in `page`. A file whose path
 * `unreachable` names as it's asked for fails to download, as with the network down: Playwright's
 * offline mode still serves the files the harness does.
 */
export async function draw(
  page: Page,
  entry: string,
  unreachable: (path: string) => boolean = () => false,
) {
  const built = await build({
    configFile: false,
    logLevel: 'warn',
    publicDir: false,
    plugins: [react()],
    build: { write: false, modulePreload: false, rolldownOptions: { input: entry } },
  });
  const { output } = (Array.isArray(built) ? built[0] : built) as Rolldown.RolldownOutput;
  const files = new Map(
    output.map((file) => [`/${file.fileName}`, file.type === 'chunk' ? file.code : file.source]),
  );
  const main = output.find((file) => file.type === 'chunk' && file.isEntry)!;
  const html = `<!doctype html><div id="root"></div><script type="module" src="/${main.fileName}"></script>`;
  await page.route(`${ORIGIN}/**`, (route) => {
    const path = new URL(route.request().url()).pathname;
    if (unreachable(path)) return route.abort('internetdisconnected');
    const body = path === '/' ? html : files.get(path);
    return body === undefined
      ? route.fulfill({ status: 404 })
      : route.fulfill({
          contentType: path === '/' ? 'text/html' : 'text/javascript',
          body: typeof body === 'string' ? body : Buffer.from(body),
        });
  });
  await page.goto(`${ORIGIN}/`);
}
