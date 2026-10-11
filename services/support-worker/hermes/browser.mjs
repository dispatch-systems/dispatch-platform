/** Private real-browser bridge. Agent input is data, never JavaScript or shell code. */
import fs from 'node:fs/promises';
import path from 'node:path';
import readline from 'node:readline';
import { pathToFileURL } from 'node:url';

const request = JSON.parse(await fs.readFile(process.argv[2], 'utf8'));
const { chromium } = await import(
  pathToFileURL(path.join(request.trustedNodeModules, 'playwright/index.mjs')).href
);
const settings = request.context.browser;
if (!settings?.url) throw new Error('No trusted browser origin supplied');
const allowed = new URL(settings.url).origin;
const browser = await chromium.launch({
  headless: true,
  args: ['--no-sandbox', '--disable-setuid-sandbox'],
});
const context = await browser.newContext({
  viewport: { width: 1440, height: 1000 },
  serviceWorkers: 'block',
  ...(settings.storageState ? { storageState: settings.storageState } : {}),
});
await context.route('**/*', async (route) => {
  const input = route.request();
  let origin;
  try {
    origin = new URL(input.url()).origin;
  } catch {
    return route.abort('blockedbyclient');
  }
  const readSession =
    input.method() === 'POST' && new URL(input.url()).pathname === '/api/session/dsp';
  if (
    origin !== allowed ||
    (settings.readOnly && !['GET', 'HEAD'].includes(input.method()) && !readSession)
  )
    return route.abort('blockedbyclient');
  await route.continue();
});
const page = await context.newPage();
context.on('page', async (popup) => {
  if (popup !== page) await popup.close();
});
await context.routeWebSocket('**/*', (socket) => socket.close());
if (settings.fixtureLogin) {
  if (settings.readOnly || !['127.0.0.1', 'localhost'].includes(new URL(settings.url).hostname))
    throw new Error('Fixture login is restricted to the isolated native data preview');
  const response = await context.request.post(`${allowed}/api/auth/login`, {
    headers: { origin: allowed },
    data: { email: 'owner@dispatch.test', password: 'Dispatch-demo-2026!' },
  });
  if (!response.ok()) throw new Error('Isolated fixture authentication failed');
}
await context.tracing.start({ screenshots: true, snapshots: true });
const artifactRoot = path.join(request.home, 'evidence');
await fs.mkdir(artifactRoot, { recursive: true, mode: 0o700 });
const ledger = {
  sourceSha: request.context.headSha,
  browser: 'chromium',
  origin: allowed,
  readOnly: settings.readOnly,
  steps: [],
  assertions: [],
};
const ledgerPath = path.join(request.home, 'witness.json');
const save = async () => {
  await fs.writeFile(`${ledgerPath}.new`, JSON.stringify(ledger), { mode: 0o600 });
  await fs.rename(`${ledgerPath}.new`, ledgerPath);
};
const locate = (input) =>
  input.role
    ? page.getByRole(input.role, { name: input.name, exact: true })
    : input.selector
      ? page.locator(input.selector)
      : page.locator('body');
await page.goto(settings.url, { waitUntil: 'domcontentloaded' });
ledger.steps.push(`Navigate ${allowed}`);
await save();

const lines = readline.createInterface({ input: process.stdin });
for await (const line of lines) {
  let answer;
  try {
    const input = JSON.parse(line);
    switch (input.action) {
      case 'navigate': {
        const url = new URL(input.url, settings.url);
        if (url.origin !== allowed) throw new Error('Navigation leaves the assigned origin');
        await page.goto(url.href, { waitUntil: 'domcontentloaded' });
        break;
      }
      case 'snapshot':
        answer = {
          url: page.url(),
          snapshot: (await page.locator('body').ariaSnapshot()).slice(0, 100000),
        };
        break;
      case 'click':
        await locate(input).click();
        break;
      case 'fill':
        await locate(input).fill(input.value);
        break;
      case 'press':
        await locate(input).press(input.value);
        break;
      case 'reload':
        await page.reload({ waitUntil: 'domcontentloaded' });
        break;
      case 'viewport': {
        if (
          !Number.isInteger(input.width) ||
          !Number.isInteger(input.height) ||
          input.width < 320 ||
          input.width > 1920 ||
          input.height < 320 ||
          input.height > 1280
        )
          throw new Error('Viewport must fit the supported mobile/desktop fixture range');
        await page.setViewportSize({ width: input.width, height: input.height });
        answer = { viewport: page.viewportSize() };
        break;
      }
      case 'upload': {
        if (
          settings.readOnly ||
          !['127.0.0.1', 'localhost'].includes(new URL(settings.url).hostname)
        )
          throw new Error('File upload is restricted to the assigned isolated preview');
        // A fresh synthetic screenshot is the only file source; no model-supplied host path.
        const fixture = await page.screenshot({ fullPage: false });
        if (fixture.byteLength > 4 * 1024 * 1024)
          throw new Error('Fixture screenshot is too large');
        await locate(input).setInputFiles({
          name: 'support-verification.png',
          mimeType: 'image/png',
          buffer: fixture,
        });
        answer = { uploadedFixture: 'support-verification.png', bytes: fixture.byteLength };
        break;
      }
      case 'download': {
        const [download] = await Promise.all([
          page.waitForEvent('download'),
          locate(input).click(),
        ]);
        const stream = await download.createReadStream();
        if (!stream) throw new Error('Download did not produce a readable file');
        let size = 0;
        const chunks = [];
        for await (const chunk of stream) {
          size += chunk.byteLength;
          if (size > 10 * 1024 * 1024) {
            await download.cancel();
            throw new Error('Verification download exceeds local artifact capacity');
          }
          chunks.push(chunk);
        }
        const bytes = Buffer.concat(chunks);
        const name = `download-${Date.now()}.bin`;
        await fs.writeFile(path.join(artifactRoot, name), bytes, { mode: 0o600 });
        const csv = download.suggestedFilename().toLowerCase().endsWith('.csv');
        ledger.lastDownload = {
          filename: download.suggestedFilename(),
          bytes: size,
          artifact: name,
          ...(csv
            ? {
                headers: bytes.toString('utf8').split(/\r?\n/, 1)[0].slice(0, 1000),
                rows: Math.max(0, bytes.toString('utf8').trim().split(/\r?\n/).length - 1),
              }
            : {}),
        };
        answer = ledger.lastDownload;
        break;
      }
      case 'screenshot': {
        const name = `${ledger.steps.length}-${Date.now()}.png`;
        await page.screenshot({ path: path.join(artifactRoot, name), fullPage: true });
        answer = { artifact: name };
        break;
      }
      case 'assert': {
        if (typeof input.criterion !== 'string' || !input.criterion.trim())
          throw new Error('Assertion needs an acceptance criterion');
        const criteria = request.context.acceptanceCriteria;
        if (criteria?.length && !criteria.includes(input.criterion))
          throw new Error('Criterion is outside the approved proposal');
        const locator = locate(input);
        let actual;
        let passed;
        switch (input.assertion) {
          case 'visible':
            actual = await locator.isVisible();
            passed = actual === true;
            break;
          case 'hidden':
            actual = await locator.isHidden();
            passed = actual === true;
            break;
          case 'text':
            actual = await locator.innerText();
            passed = actual.includes(input.expected);
            break;
          case 'value':
            actual = await locator.inputValue();
            passed = actual === input.expected;
            break;
          case 'url':
            actual = page.url();
            passed = actual === new URL(input.expected, settings.url).href;
            break;
          case 'count':
            actual = await locator.count();
            passed = String(actual) === input.expected;
            break;
          case 'download':
            actual = ledger.lastDownload;
            passed = Boolean(
              actual?.bytes &&
              (actual.filename.includes(input.expected) ||
                actual.headers?.includes(input.expected)),
            );
            break;
          default:
            throw new Error('Unknown browser assertion');
        }
        const screenshot = `${ledger.assertions.length}-${Date.now()}.png`;
        await page.screenshot({ path: path.join(artifactRoot, screenshot), fullPage: true });
        const assertion = {
          criterion: input.criterion,
          passed,
          assertion: input.assertion,
          expected: input.expected,
          actual,
          screenshot,
          steps: [...ledger.steps, `Assert ${input.assertion} for ${input.criterion}`],
          url: page.url(),
        };
        ledger.assertions.push(assertion);
        answer = assertion;
        break;
      }
      default:
        throw new Error('Unsupported browser action');
    }
    ledger.steps.push(
      `${input.action}${input.role ? ` role ${input.role} ${input.name ?? ''}` : input.selector ? ` selector ${input.selector}` : ''}`,
    );
    await save();
    answer ??= { ok: true, url: page.url() };
  } catch (error) {
    answer = { error: error.message };
  }
  process.stdout.write(`${JSON.stringify(answer)}\n`);
}
ledger.finalScreenshot = `observation-${Date.now()}.png`;
await page.screenshot({ path: path.join(artifactRoot, ledger.finalScreenshot), fullPage: true });
await save();
await context.tracing.stop({ path: path.join(artifactRoot, 'trace.zip') });
await browser.close();
