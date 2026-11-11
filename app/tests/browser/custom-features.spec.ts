import type { Page, Route } from '@playwright/test';
import {
  test,
  expect,
  login,
  openDsp,
  signIn,
  demo,
  dspAddress,
  linkIn,
  type Dispatch,
} from '../../../core/shell/tests/support/fixtures.js';
import { capturedMail } from '../../../core/shell/tests/support/mail-support.js';
import type { AuditPage } from '../../../core/platform_owner/api/index.js';

const OWNER_CODE = 'fscl';

// The server's owner metadata and private-asset access have HTTP coverage; the build has a
// real two-owner bundle test. This stand-in exercises their browser admission and slots
// without adding a custom feature to the product build.
async function custom(page: Page, dispatch: Dispatch, hold = false) {
  const platform = await dispatch.client();
  const own = platform.session.dsps.find((dsp: { code: string }) => dsp.code === demo.memberDsp);
  const coded = await platform.post(`/api/platform/dsps/${own.id}/code`, { code: OWNER_CODE });
  expect(coded.status).toBe(200);
  own.code = OWNER_CODE;
  const id = `${own.code}_fuel_log`;
  const routeId = `${own.code}-fuel-log`;
  const entry = `/assets/custom-${own.code}-fuel-log-12345678.js`;
  const platformEntry = '/assets/owner-browser-catalog-12345678.js';
  const style = `/assets/custom-${own.code}-fuel-log-12345678.css`;
  const lazy = `/assets/custom-${own.code}-fuel-page-12345678.js`;
  let authenticated = false;
  // Model the server's audience-scoped document policy separately from API admission.
  await page.route('**/*', async (route) => {
    if (route.request().resourceType() !== 'document') return route.fallback();
    const response = await route.fetch();
    await route.fulfill({
      response,
      headers: {
        ...response.headers(),
        'content-security-policy': `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:${authenticated ? ' https://owner-media.example.test' : ''}`,
      },
    });
  });
  await page.route('**/api/auth/login', async (route) => {
    const response = await route.fetch();
    if (response.ok()) authenticated = true;
    await route.fulfill({ response });
  });
  await page.route('https://owner-media.example.test/probe.svg', (route) =>
    route.fulfill({
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>',
    }),
  );
  await page.route('**/api/session', async (route) => {
    const response = await route.fetch();
    const session = await response.json();
    if (response.ok() && session.user.platformOwner) session.platformFrontend = platformEntry;
    await route.fulfill({ response, json: session });
  });
  await page.route(`**${platformEntry}`, (route) =>
    route.fulfill({
      contentType: 'text/javascript',
      body: `export const featureLabels = ${JSON.stringify({ [id]: 'Fuel Log' })};
      export const permissionLabels = ${JSON.stringify({ [`${id}.view`]: 'View Fuel Log' })};
      export const features = [{ name: ${JSON.stringify(id)}, platformSlots: async () => ({ slots: {
        auditWording: { phrases: { ${JSON.stringify(`${id}.recorded`)}: () => ['logged fuel'] } }
      } }) }];`,
    }),
  );
  await page.route('**/api/platform/audit?*', (route) =>
    route.fulfill({
      json: {
        events: [
          {
            id: 2,
            at: '2026-09-16T14:44:00Z',
            actorId: 'usr_fixture',
            actorName: 'Fixture owner',
            dspId: own.id,
            dspName: own.name,
            action: `${id}.recorded`,
            area: 'collections',
            detail: '',
            target: null,
            ref: null,
            changes: [],
          },
          {
            id: 1,
            at: '2026-09-16T14:43:00Z',
            actorId: 'usr_fixture',
            actorName: 'Fixture owner',
            dspId: own.id,
            dspName: own.name,
            action: 'dsp.feature_enabled',
            area: 'settings',
            detail: id,
            target: null,
            ref: null,
            changes: [{ field: 'permission', from: null, to: `${id}.view` }],
          },
        ],
        total: 2,
        counts: { collections: 1, settings: 1 },
        actors: [],
        dsps: [{ id: own.id, name: own.name }],
      } satisfies AuditPage,
    }),
  );
  const module = `const sourceStyle = document.createElement('style');
    sourceStyle.dataset.viteDevId = '/features/custom/${own.code}/fuel_log/frontend/feature.css';
    sourceStyle.textContent = ':root { --owner-vite-style: 1; }';
    document.head.append(sourceStyle);
    export const feature = { name: '${id}', routes: [{
    id: '${routeId}', scope: 'dsp', label: 'Fuel Log', nav: true, feature: '${id}',
    permission: ({view}) => view.permissions.includes('${id}.view'),
    preload: () => import('${lazy}'), render: () => 'Fuel entries'
  }] }; export const peopleKinds = []; export const peopleSources = []; export const peopleFeatures = [];`;
  await page.route('**/api/session/dsp', async (route) => {
    const response = await route.fetch();
    const view = await response.json();
    if (response.ok() && view.dsp.id === own.id) {
      view.features.push(id);
      view.permissions.push(`${id}.view`);
      view.custom = {
        catalog: [
          {
            id,
            label: 'Fuel Log',
            place: 500,
            kind: 'page',
            permissions: [`${id}.view`],
            requires: [],
            page: null,
            tab: false,
            connects: false,
          },
        ],
        permissions: [
          { id: `${id}.view`, label: 'View Fuel Log', group: null, implied: [], under: null },
        ],
        frontends: [{ name: id, url: entry, styles: [style], place: 500 }],
      };
    }
    await route.fulfill({ response, json: view });
  });
  let release!: (route: Route) => void;
  const held = new Promise<Route>((resolve) => {
    release = resolve;
  });
  await page.route(`**${entry}`, async (route) => {
    if (hold) release(route);
    else await route.fulfill({ contentType: 'text/javascript', body: module });
  });
  await page.route(`**${lazy}`, (route) =>
    route.fulfill({ contentType: 'text/javascript', body: 'export const ready = true;' }),
  );
  await page.route(`**${style}`, (route) =>
    route.fulfill({ contentType: 'text/css', body: ':root { --owner-fuel-style: 1; }' }),
  );
  return { own, id, entry, lazy, held, module, platform };
}

const ownerImage = (page: Page) =>
  page.evaluate(
    () =>
      new Promise<boolean>((resolve) => {
        const image = new Image();
        image.onload = () => resolve(true);
        image.onerror = () => resolve(false);
        image.src = 'https://owner-media.example.test/probe.svg';
      }),
  );

test('an owner sees the custom page and role choice while another DSP receives none of its code or names', async ({
  page,
  browser,
  dispatch,
}) => {
  const feature = await custom(page, dispatch);
  await page.goto('/');
  expect(await ownerImage(page)).toBe(false);
  await signIn(page);
  await openDsp(page, 'Northline Logistics');
  expect(await ownerImage(page)).toBe(true);
  const link = page.getByRole('link', { name: 'Fuel Log', exact: true });
  await expect(link).toBeVisible();
  const loaded = page.waitForResponse(
    (response) => new URL(response.url()).pathname === feature.lazy,
  );
  await link.click();
  await loaded;
  await expect(page.getByText('Fuel entries', { exact: true })).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue('--owner-fuel-style').trim(),
      ),
    )
    .toBe('1');
  expect(
    await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue('--owner-vite-style').trim(),
    ),
  ).toBe('1');
  await page.getByRole('link', { name: 'Team & Roles', exact: true }).click();
  await page.getByRole('tab', { name: 'Roles', exact: true }).click();
  await page.getByRole('button', { name: 'Create role', exact: true }).click();
  await expect(
    page
      .getByRole('dialog', { name: 'Create role' })
      .getByRole('switch', { name: 'View Fuel Log', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('button', { name: 'Exit view', exact: true }).click();
  await openDsp(page, 'Summit Delivery');
  await expect(link).toHaveCount(0);
  await expect
    .poll(() =>
      page.evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue('--owner-fuel-style').trim(),
      ),
    )
    .toBe('');
  expect(
    await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue('--owner-vite-style').trim(),
    ),
  ).toBe('');
  // Native module imports are cached. Re-entering must restore the detached development
  // styles without executing the module again, then remove them on the next boundary.
  await page.getByRole('button', { name: 'Exit view', exact: true }).click();
  await openDsp(page, 'Northline Logistics');
  await expect(link).toBeVisible();
  expect(
    await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue('--owner-vite-style').trim(),
    ),
  ).toBe('1');
  await page.getByRole('button', { name: 'Exit view', exact: true }).click();
  await openDsp(page, 'Summit Delivery');
  await expect(page.getByRole('region', { name: 'DSP viewing mode', exact: true })).toContainText(
    'Summit Delivery',
  );
  expect(
    await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue('--owner-vite-style').trim(),
    ),
  ).toBe('');

  // A real, separate DSP account gives the foreign browser no platform-owner exception.
  const created = await feature.platform.post('/api/platform/dsps', {
    name: 'Other Logistics',
    ownerEmail: 'other-owner@dispatch.test',
  });
  expect(created.status).toBe(201);
  const dsp = created.value.dsp;
  expect(
    (await feature.platform.post(`/api/platform/dsps/${dsp.id}/code`, { code: 'other' })).status,
  ).toBe(200);
  const mail = await capturedMail(dispatch.root, 'other-owner@dispatch.test');
  const token = /token=([A-Za-z0-9_-]{43})/.exec(linkIn(mail.text))![1];
  expect(
    (
      await dispatch.request(
        `/api/invitations/${token}/accept`,
        { firstName: 'Other', lastName: 'Owner', password: demo.password },
        dispatch.at('other'),
      )
    ).status,
  ).toBe(200);
  const context = await browser.newContext();
  const other = await context.newPage();
  try {
    const urls: string[] = [];
    const bodies: string[] = [];
    // Capture each response before delivering it. CDP can discard a document's response
    // body when sign-in navigates away; buffering here keeps the byte assertion complete.
    await other.route('**/*', async (route) => {
      const response = await route.fetch({
        maxRedirects: 0,
        headers: { ...route.request().headers(), 'accept-encoding': 'identity' },
      });
      const body = await response.body();
      urls.push(route.request().url());
      if (/javascript|json|css|html/.test(response.headers()['content-type'] ?? ''))
        bodies.push(body.toString('utf8'));
      await route.fulfill({ response, body });
    });
    await other.goto(dspAddress(page, 'other'));
    await signIn(other, 'other-owner@dispatch.test');
    await expect(other.getByRole('link', { name: 'Team & Roles', exact: true })).toBeVisible();
    await other.getByRole('link', { name: 'Team & Roles', exact: true }).click();
    await other.getByRole('tab', { name: 'Roles', exact: true }).click();
    await other.getByRole('button', { name: 'Create role', exact: true }).click();
    await expect(other.getByRole('dialog', { name: 'Create role' })).toBeVisible();
    await expect(other.getByRole('switch', { name: 'View Fuel Log', exact: true })).toHaveCount(0);
    const session = await dispatch.client('other-owner@dispatch.test', demo.password, 'other');
    expect(session.session.user.platformOwner).toBe(false);
    expect(session.session.dsps.map((item: { id: string }) => item.id)).toEqual([dsp.id]);
    for (const value of [...urls, ...bodies]) {
      expect(value).not.toContain(feature.id);
      expect(value).not.toContain('Fuel Log');
      expect(value).not.toContain(feature.entry);
    }
  } finally {
    await other.unrouteAll({ behavior: 'ignoreErrors' });
    await context.close();
  }
  await page.getByRole('button', { name: 'Exit view', exact: true }).click();
  await page.getByRole('link', { name: 'Audit log', exact: true }).click();
  await expect(
    page.getByRole('listitem').filter({ hasText: 'Fixture owner logged fuel' }),
  ).toBeVisible();
  await expect(page.getByRole('listitem').filter({ hasText: 'enabled Fuel Log' })).toContainText(
    'View Fuel Log',
  );
});

test('a delayed custom import cannot install its page or styles into the next DSP', async ({
  page,
  dispatch,
}) => {
  const feature = await custom(page, dispatch, true);
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const held = await feature.held;
  await page.goBack();
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'All DSPs', exact: true }).click();
  await openDsp(page, 'Summit Delivery');
  await expect(page.getByRole('region', { name: 'DSP viewing mode', exact: true })).toContainText(
    'Summit Delivery',
  );
  const finished = page.waitForResponse(
    (response) => new URL(response.url()).pathname === feature.entry,
  );
  await held.fulfill({ contentType: 'text/javascript', body: feature.module });
  await finished;
  await page.evaluate(async (url) => {
    await import(url);
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }, feature.entry);
  await expect(page.getByRole('link', { name: 'Fuel Log', exact: true })).toHaveCount(0);
  await expect(page.getByText('Fuel entries', { exact: true })).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue('--owner-vite-style').trim(),
    ),
  ).toBe('');
  expect(
    await page.evaluate(() =>
      [...document.querySelectorAll<HTMLLinkElement>('link[rel="stylesheet"]')].some((link) =>
        new URL(link.href).pathname.startsWith('/assets/custom-'),
      ),
    ),
  ).toBe(false);
});
