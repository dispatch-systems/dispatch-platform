// A page of a running preview as a PNG, for `dispatchdev shot`, which signs the account in and
// hands this the session: the browser tests' Chromium opens the page at the size and in the
// theme it is told, on the tab it is told, and saves what it shows once the page is ready.
import { parseArgs } from 'node:util';
import { chromium, type Page } from '@playwright/test';

const { values } = parseArgs({
  options: {
    url: { type: 'string' },
    /** The session, as a Playwright storage state file. */
    storage: { type: 'string' },
    /** The dev server's host, which answers every DSP's own address under localhost. */
    host: { type: 'string' },
    size: { type: 'string' },
    theme: { type: 'string' },
    /** A tab of the page by its id: the address opens Settings' pages, and this clicks the rest. */
    tab: { type: 'string' },
    out: { type: 'string' },
  },
});
const [width, height] = values.size!.split('x').map(Number) as [number, number];

/**
 * Resolves once no request has started or ended for half a second. A DSP's pages keep one open
 * for live updates, so the page is never without one.
 */
function settled(page: Page) {
  return new Promise<void>((resolve) => {
    let timer = setTimeout(done, 500);
    function wait() {
      clearTimeout(timer);
      timer = setTimeout(done, 500);
    }
    function done() {
      page.off('request', wait).off('requestfinished', wait).off('requestfailed', wait);
      resolve();
    }
    page.on('request', wait).on('requestfinished', wait).on('requestfailed', wait);
  });
}

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
  // Ready once nothing has loaded for a while, its fonts are in and nothing on it is loading.
  const ready = async () => {
    await settled(page);
    await page.waitForFunction(
      () =>
        document.fonts.status === 'loaded' &&
        !document.querySelector('.loading, [aria-busy="true"]'),
    );
  };
  await page.goto(values.url!);
  await ready();
  if (values.tab) {
    const tab = page.locator(`[role="tab"][data-tab=${JSON.stringify(values.tab)}]`);
    if (await tab.count()) {
      if ((await tab.getAttribute('aria-selected')) !== 'true') {
        await tab.click();
        await ready();
      }
    } else if (await page.locator('[role="tab"]').count()) {
      console.log(`The page has no tab ${values.tab}: it shows the one it opens on.`);
    }
  }
  await page.screenshot({ path: values.out! });
} finally {
  await browser.close();
}
