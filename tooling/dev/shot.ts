// A page of a running preview as a PNG, for `dispatchdev shot`, which signs the account in and
// hands this the session: the browser tests' Chromium opens the page at the size and in the
// theme it is told, on the tab it is told, clicks what it is told to, and saves what it shows
// once the page is ready, a pixel of the PNG to a pixel of the window.
import { parseArgs } from 'node:util';
import { chromium, type Locator, type Page } from '@playwright/test';

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
    /** A control by its accessible name, clicked once the page is ready, each in turn. */
    click: { type: 'string', multiple: true, default: [] },
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

/** What a person clicks: a button, link, tab, menu's item, list's option, row or toggle. */
const CLICKABLE = [
  'button',
  'link',
  'tab',
  'menuitem',
  'option',
  'row',
  'checkbox',
  'radio',
  'switch',
] as const;
/** The one control shown that `name` names: by its whole accessible name, else by a part of it. */
async function control(page: Page, name: string): Promise<Locator> {
  for (const exact of [true, false]) {
    const named = CLICKABLE.map((role) => page.getByRole(role, { name, exact }))
      .reduce((all, each) => all.or(each))
      .filter({ visible: true });
    const count = await named.count();
    if (count === 1) return named;
    if (count > 1) throw new Error(`${count} controls are named ${JSON.stringify(name)}.`);
  }
  throw new Error(`Nothing to click is named ${JSON.stringify(name)}.`);
}

const browser = await chromium.launch({
  args: [`--host-resolver-rules=MAP *.localhost ${values.host}`],
});
try {
  const context = await browser.newContext({
    storageState: values.storage,
    viewport: { width, height },
    deviceScaleFactor: 1,
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
        console.log(
          `The address doesn't open the tab ${values.tab}: a link lands a click from it.`,
        );
        await tab.click();
        await ready();
      }
    } else if (await page.locator('[role="tab"]').count()) {
      console.log(`The page has no tab ${values.tab}: it shows the one it opens on.`);
    }
  }
  for (const name of values.click) {
    await (await control(page, name)).click();
    await ready();
  }
  await page.screenshot({ path: values.out! });
} catch (error) {
  // Said alone, for `dispatchdev` to pass on.
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
} finally {
  await browser.close();
}
