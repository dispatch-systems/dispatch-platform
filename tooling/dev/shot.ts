// A page of a running preview as a PNG, for `dispatchdev shot`, which signs the account in and
// hands this the session: the browser tests' Chromium opens the page at the size and in the
// theme it is told, and saves what it shows once the page is ready.
import { parseArgs } from 'node:util';
import { chromium } from '@playwright/test';

const { values } = parseArgs({
  options: {
    url: { type: 'string' },
    /** The session, as a Playwright storage state file. */
    storage: { type: 'string' },
    /** The preview's host, which answers every DSP's own address under localhost. */
    host: { type: 'string' },
    size: { type: 'string' },
    theme: { type: 'string' },
    out: { type: 'string' },
  },
});
const [width, height] = values.size!.split('x').map(Number) as [number, number];

const browser = await chromium.launch({
  args: [`--host-resolver-rules=MAP *.localhost ${values.host}`],
});
try {
  const context = await browser.newContext({
    storageState: values.storage,
    viewport: { width, height },
    colorScheme: values.theme as 'light' | 'dark',
  });
  const page = await context.newPage();
  // Ready once its requests have answered, its fonts are in and nothing on it is still loading.
  await page.goto(values.url!, { waitUntil: 'networkidle' });
  await page.waitForFunction(
    () =>
      document.fonts.status === 'loaded' && !document.querySelector('.loading, [aria-busy="true"]'),
  );
  await page.screenshot({ path: values.out! });
} finally {
  await browser.close();
}
