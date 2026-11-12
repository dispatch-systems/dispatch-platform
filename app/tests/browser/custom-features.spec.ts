import fs from 'node:fs';
import type { Page, Request, Route } from '@playwright/test';
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
import { virtualAuthenticator } from '../../../core/accounts/tests/support/authenticator.js';
import type {
  AuditPage,
  DspFeatureReport,
  Visibility,
} from '../../../core/platform_owner/api/index.js';

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
  const platformStyle = `/assets/custom-${own.code}-platform-12345678.css`;
  const style = `/assets/custom-${own.code}-fuel-log-12345678.css`;
  const lazy = `/assets/custom-${own.code}-fuel-page-12345678.js`;
  const catalog = [
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
  ];
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
      export const features = [{ name: ${JSON.stringify(id)}, platformSlots: async () => {
        const style = document.createElement('style');
        style.dataset.viteDevId = '/features/custom/${own.code}/fuel_log/frontend/platform.css';
        style.textContent = ':root { --owner-platform-source: 1; }';
        document.head.append(style);
        await new Promise((resolve) => {
          const link = document.createElement('link');
          link.rel = 'stylesheet'; link.href = ${JSON.stringify(platformStyle)};
          link.onload = resolve; document.head.append(link);
        });
        return { slots: {
        auditWording: { phrases: { ${JSON.stringify(`${id}.recorded`)}: () => ['logged fuel'] } }
      } }; } }];`,
    }),
  );
  await page.route(`**${platformStyle}`, (route) =>
    route.fulfill({ contentType: 'text/css', body: ':root { --owner-platform-style: 1; }' }),
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
        catalog,
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
  return { own, id, entry, lazy, held, module, platform, platformEntry, catalog };
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
  expect(
    await page.evaluate(() => {
      const style = getComputedStyle(document.documentElement);
      return ['--owner-platform-source', '--owner-platform-style'].map((name) =>
        style.getPropertyValue(name).trim(),
      );
    }),
  ).toEqual(['1', '1']);
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

test("the platform lists custom controls only on their owning DSP's Features tab", async ({
  page,
  dispatch,
}) => {
  const feature = await custom(page, dispatch);
  const own = feature.own.id;
  const part = `${feature.id}.entries`;
  const catalog = [
    ...feature.catalog.map((entry) => ({ ...entry, requires: ['routes'] })),
    {
      ...feature.catalog[0]!,
      id: part,
      label: 'Entries',
      kind: 'sub',
      page: feature.id,
      tab: true,
      permissions: [],
    },
  ];
  const ids = catalog.map((entry) => entry.id);
  const summarize = (dsps: { id: string; customFeatures?: string[] }[]) => {
    for (const dsp of dsps) if (dsp.id === own) dsp.customFeatures = ids;
  };
  await page.route('**/api/session', async (route) => {
    const response = await route.fetch();
    const session = await response.json();
    if (response.ok()) {
      session.platformFrontend = feature.platformEntry;
      summarize(session.dsps);
    }
    await route.fulfill({ response, json: session });
  });
  await page.route('**/api/platform/dsps', async (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    const response = await route.fetch();
    const dsps = await response.json();
    summarize(dsps);
    await route.fulfill({ response, json: dsps });
  });
  let enabled = false;
  let visibility: Visibility = 'shown';
  const states = () =>
    ids.map((id) => ({
      feature: id,
      enabled: id === part || enabled,
      visibility: id === part ? ('shown' as const) : visibility,
      changedAt: null,
      changedBy: null,
    }));
  await page.route(`**/api/platform/dsps/${own}/features`, async (route) => {
    if (route.request().method() === 'POST') {
      const requested = route.request().postDataJSON();
      expect(requested.feature).toBe(feature.id);
      enabled = requested.enabled;
      return route.fulfill({
        json: {
          features: enabled ? ids : [part],
          changed: [{ feature: feature.id, enabled }],
        },
      });
    }
    const response = await route.fetch();
    const report: DspFeatureReport = await response.json();
    await route.fulfill({
      response,
      json: {
        ...report,
        features: [...report.features, ...states()],
        custom: catalog,
      },
    });
  });
  await page.route(`**/api/platform/dsps/${own}/features/visibility`, (route) => {
    const requested = route.request().postDataJSON();
    expect(requested.feature).toBe(feature.id);
    visibility = requested.visibility;
    return route.fulfill({
      json: {
        hidden: visibility === 'hidden' ? [feature.id] : [],
        beta: visibility === 'beta' ? [feature.id] : [],
      },
    });
  });
  await login(page);
  await page
    .getByRole('region', { name: 'DSPs', exact: true })
    .getByRole('button', { name: /Northline Logistics/ })
    .click();
  const pane = page.getByRole('region', {
    name: 'Northline Logistics',
    exact: true,
  });
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  const customGroup = pane.getByRole('group', { name: 'Custom', exact: true });
  await expect(customGroup).toBeVisible();
  await customGroup.getByRole('button', { name: /Fuel Log/ }).click();
  const toggle = pane.getByRole('switch', { name: 'Fuel Log', exact: true });
  await expect(toggle).not.toBeChecked();
  await toggle.check();
  await expect(toggle).toBeChecked();
  await expect(pane.getByRole('switch', { name: 'Entries tab', exact: true })).toBeChecked();
  const sight = pane.getByRole('radiogroup', {
    name: 'Who sees Fuel Log',
    exact: true,
  });
  await sight.getByRole('radio', { name: 'Beta testers', exact: true }).click();
  await expect(sight.getByRole('radio', { name: 'Beta testers', exact: true })).toBeChecked();
  await sight.getByRole('radio', { name: 'Hidden', exact: true }).click();
  await expect(sight.getByRole('radio', { name: 'Hidden', exact: true })).toBeChecked();
  // Switching DSPs must immediately discard the previous report's private names and section.
  await pane.getByRole('button', { name: 'Switch DSP', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Switch DSP', exact: true })
    .getByRole('button', { name: /Summit Delivery/ })
    .click();
  const other = page.getByRole('region', {
    name: 'Summit Delivery',
    exact: true,
  });
  await expect(other.getByRole('navigation', { name: 'Features', exact: true })).toBeVisible();
  await expect(other.getByRole('group', { name: 'Custom', exact: true })).toHaveCount(0);
  await expect(other.getByText('Fuel Log', { exact: true })).toHaveCount(0);

  await other.getByRole('button', { name: 'Switch DSP', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Switch DSP', exact: true })
    .getByRole('button', { name: /Northline Logistics/ })
    .click();
  await expect(toggle).toBeChecked();
  expect(
    (
      await feature.platform.post(`/api/platform/dsps/${own}/features`, {
        feature: 'cortex',
        enabled: false,
      })
    ).status,
  ).toBe(200);
  await toggle.uncheck();
  const cortex = pane
    .getByRole('navigation', { name: 'Features', exact: true })
    .getByRole('button')
    .filter({ has: page.getByText('Cortex', { exact: true }) });
  await expect(cortex).toContainText('Off');
  await toggle.click();
  await expect(
    page.getByRole('dialog', { name: 'Switch on Fuel Log?', exact: true }),
  ).toBeVisible();
  // Browser navigation can leave a confirmation open without using the DSP switcher.
  const foreign = feature.platform.session.dsps.find((dsp: { code: string }) => dsp.code === 'sum');
  const address = new URL(page.url());
  address.hash = `dsps?dsp=${foreign.id}&feature=${feature.id}`;
  await page.goto(address.href);
  await expect(other.getByRole('navigation', { name: 'Features', exact: true })).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Switch on Fuel Log?', exact: true })).toHaveCount(
    0,
  );
  await expect(other.getByRole('group', { name: 'Custom', exact: true })).toHaveCount(0);
});

test('recent verification keeps replacement recovery codes available to save', async ({
  page,
  dispatch,
}) => {
  await custom(page, dispatch);
  await virtualAuthenticator(page);
  await login(page);
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByRole('tab', { name: 'Security', exact: true }).click();
  await page.getByRole('button', { name: 'Add passkey', exact: true }).click();
  const adding = page.getByRole('dialog', { name: 'Add passkey', exact: true });
  await adding.getByLabel('Passkey name').fill('Fixture security key');
  await adding.getByRole('button', { name: 'Add passkey', exact: true }).click();
  const heading = page.getByRole('heading', { name: 'Save your recovery codes', exact: true });
  await expect(heading).toBeVisible();
  const refreshed = page.waitForResponse('**/api/session');
  await page.getByRole('button', { name: 'I saved my recovery codes', exact: true }).click();
  await (await refreshed).finished();
  // Only this fixture's account verification ages; no real clock or live account changes.
  dispatch.database('data/platform/accounts.sqlite', (db) => {
    db.exec('UPDATE session_security SET verified_at=verified_at-301000');
  });
  const navigations: string[] = [];
  page.on('request', (request) => {
    if (request.isNavigationRequest()) navigations.push(request.url());
  });
  let releaseSession!: () => void;
  const sessionsHeld = new Promise<void>((resolve) => {
    releaseSession = resolve;
  });
  let holdingSessions = true;
  const sessionReads: Request[] = [];
  await page.route('**/api/session', async (route) => {
    if (holdingSessions) {
      sessionReads.push(route.request());
      await sessionsHeld;
    }
    await route.fallback();
  });
  let releaseCodes!: () => void;
  const codesHeld = new Promise<void>((resolve) => {
    releaseCodes = resolve;
  });
  let created!: (codes: string[]) => void;
  const replacement = new Promise<string[]>((resolve) => {
    created = resolve;
  });
  await page.route('**/api/auth/security/recovery-codes', async (route) => {
    const response = await route.fetch();
    if (response.ok()) {
      created((await response.json()).codes);
      await codesHeld;
    }
    await route.fulfill({ response });
  });
  try {
    await page.getByRole('button', { name: 'Replace recovery codes', exact: true }).click();
    const prompt = page.getByRole('dialog', { name: 'Confirm it’s you', exact: true });
    await expect(prompt).toBeVisible();
    await prompt.getByRole('button', { name: 'Verify with passkey', exact: true }).click();
    const codes = await replacement;
    expect(codes).toHaveLength(10);
    // The server has replaced the saved codes. Finish any admission refresh before
    // releasing their response: it must preserve the document awaiting those codes.
    holdingSessions = false;
    releaseSession();
    await Promise.all(sessionReads.map(async (request) => (await request.response())?.finished()));
    await page.evaluate(
      () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())),
    );
    releaseCodes();
    expect(navigations).toEqual([]);
    await expect(heading).toBeVisible();
    await expect(page.getByLabel('Formatted recovery codes')).toContainText(codes[0]!);
    const downloading = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Download', exact: true }).click();
    const download = await downloading;
    const file = await download.path();
    if (!file) throw new Error('Recovery codes were not downloaded');
    const saved = fs.readFileSync(file, 'utf8');
    for (const code of codes) expect(saved).toContain(code);
  } finally {
    releaseCodes();
    releaseSession();
    await page.unrouteAll({ behavior: 'ignoreErrors' });
  }
});
