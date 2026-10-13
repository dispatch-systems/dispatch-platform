import { test, expect, demo, login } from '../../../core/shell/tests/support/fixtures.js';

test('startup reads the session while the address is still loading', async ({ page }) => {
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/site', async (route) => {
    await held;
    await route.continue();
  });
  const session = page.waitForRequest('**/api/session');
  try {
    await page.goto('/');
    await session;
  } finally {
    release();
  }
  await expect(page.getByRole('heading', { name: 'Sign in', exact: true })).toBeVisible();
});

test('an idle dashboard renews an expired presence view once and signs out when its session expires', async ({
  page,
  dispatch,
}) => {
  const member = await dispatch.client(demo.member);
  const dsp = member.session.dsps[0];
  let opened = 0,
    refused = 0,
    accepted = 0;
  page.on('response', (response) => {
    const path = new URL(response.url()).pathname;
    if (path === '/api/session/dsp' && response.status() === 200) opened++;
    if (path === '/api/dsp/presence' && response.request().postDataJSON().state !== 'gone') {
      if (response.status() === 409) refused++;
      if (response.status() === 200) accepted++;
    }
  });
  await page.clock.install();
  await login(page, demo.member);
  await expect(page.getByRole('heading', { name: 'Currently under development' })).toBeVisible();
  await expect.poll(() => accepted).toBeGreaterThan(0);
  const first = opened;
  dispatch.database('data/platform/accounts.sqlite', (db) =>
    db.prepare('UPDATE dsps SET revision=revision+1 WHERE id=?').run(dsp.id),
  );
  await page.clock.runFor(35_000);
  await expect.poll(() => opened).toBe(first + 1);
  await expect.poll(() => accepted).toBeGreaterThan(1);
  await page.clock.runFor(35_000);
  expect(refused).toBe(1);
  dispatch.people(dsp.id, (db) => db.exec('UPDATE sessions SET expires_at=0'));
  await page.clock.runFor(35_000);
  await expect(page.getByRole('heading', { name: 'Sign in', exact: true })).toBeVisible();
});
