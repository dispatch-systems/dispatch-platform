import type { Route } from '@playwright/test';
import {
  test,
  expect,
  demo,
  login,
  openDsp,
} from '../../../../core/shell/tests/support/fixtures.js';
import type { PhoneApp, PhonePairing, Phones } from '../../api/index.js';

// The seeded DSP has every feature switched on, its demo's stand-in phone paired, and its owner
// holds every permission.

test('Add phone closes once its own code pairs a phone, and a rename asks before it closes', async ({
  page,
  dispatch,
}) => {
  await page.clock.install();
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Connections', exact: true })
    .click();
  const card = page
    .getByRole('article')
    .filter({ has: page.getByRole('heading', { name: 'Phones', level: 3 }) });
  const list = card.getByRole('list', { name: 'Phones' });
  await expect(list).toContainText('(206) 555-0100');
  // The code the dialog makes, as the server answers it.
  const made = async () =>
    (await (
      await page.waitForResponse((response) =>
        response.url().endsWith('/api/dsp/communications/phones/pairing'),
      )
    ).json()) as PhonePairing;
  let making = made();
  await card.getByRole('button', { name: 'Add phone' }).click();
  const first = await making;
  const dialog = page.getByRole('dialog', { name: 'Add phone' });
  const picture = dialog.getByRole('img', { name: /^Pairing code [A-HJ-NP-Z2-9]{8}$/ });
  await expect(picture).toBeVisible();
  await expect(dialog).toContainText(/Expires in \d+:\d\d/);
  // A phone pairs as its app does, at the DSP's own address.
  const pair = async (code: string, number: string) => {
    const paired = await dispatch.request(
      '/api/communications/phone/pair',
      { code, number, model: 'Pixel 8a', version: '1.0.0' },
      dispatch.at(demo.memberDsp),
    );
    expect(paired.status, paired.body).toBe(200);
  };

  // Another session's newer code replaces it, and pairs a phone: this one gives way to a new code.
  const owner = await dispatch.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const theirs = await owner.post('/api/dsp/communications/phones/pairing');
  await pair(theirs.value.code, '(206) 555-0177');
  await page.clock.fastForward(3_000);

  // Its own code pairing a phone closes it, and the card shows that phone, though it paired after
  // the dialog last asked, a second before the code expires. The dialog's asks after that code
  // wait here, each answered by the server once let through.
  const asks: Route[] = [];
  await page.route('**/api/dsp/communications/phones/pairing/*', (route) =>
    route.request().url().endsWith(`/${first.id}`) ? route.continue() : void asks.push(route),
  );
  const answer = async () => {
    await expect.poll(() => asks.length, { message: 'the dialog asks after its code' }).toBe(1);
    const ask = asks.pop()!;
    await ask.fulfill({ response: await ask.fetch() });
  };
  making = made();
  await dialog.getByRole('button', { name: 'New code' }).click();
  const own = await making;
  // Held still past how often it asks, though well within how long an ask may wait, its next ask
  // and tick come a whole period on. The time then reads a second before the code expires, with
  // nothing fired: a jump there would time out the ask held, which is then asked again.
  await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 4_000);
  await page.clock.setSystemTime(own.expiresAt - 1_000);
  await answer();
  await pair(own.code, '(206) 555-0188');
  await page.clock.runFor(1_000);
  await answer();
  await expect(dialog).toBeHidden();
  await page.clock.resume();
  await expect(list).toContainText('(206) 555-0188');

  // A name changed and not saved is never just closed.
  await card.getByLabel('Actions for Dispatch').click();
  await page.getByRole('button', { name: 'Rename', exact: true }).click();
  const rename = page.getByRole('dialog', { name: 'Rename phone' });
  await rename.getByLabel('Name').fill('Station 2');
  await page.keyboard.press('Escape');
  const asking = page.getByRole('dialog', { name: 'Save changes?' });
  await expect(asking.getByRole('button')).toHaveText(['Discard changes', 'Save changes']);
});

test('The card offers the app its DSP hands out: its download, an update to an older one, and in Add phone, to install first', async ({
  page,
  dispatch,
}) => {
  // The server's answer, with the app a runtime's build packed in it.
  const app: PhoneApp = {
    name: 'DSP Dispatch: Relay Dev',
    version: '1.4.0',
    link: `http://${dispatch.at(demo.memberDsp).host}/api/communications/phone/app/download`,
    picture: 'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciLz4=',
  };
  const lastSeenAt = Date.now() - 3 * 60_000;
  // Its stand-in phone runs an older version than that and has been offline for three minutes.
  await page.route('**/api/dsp/communications/phones', async (route) => {
    const response = await route.fetch();
    const data = (await response.json()) as Phones;
    // Long details, which the row cuts short, never hide the tag: a model as long as a phone may
    // report (`MODEL_MOST`).
    const older = data.phones.map((phone) => ({
      ...phone,
      name: 'Dispatch phone at the north station, by the loading dock',
      model: `Samsung Galaxy ${'A'.repeat(85)}`,
      appVersion: '1.3.0',
      update: true,
      online: false,
      lastSeenAt,
      health: {
        cantSend: false,
        cantRead: false,
        noSim: true,
        noSignal: false,
        setupUndone: false,
        waiting: 0,
        undownloaded: null,
      },
    }));
    await route.fulfill({ response, json: { ...data, phones: older, app } });
  });
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Connections', exact: true })
    .click();
  const card = page
    .getByRole('article')
    .filter({ has: page.getByRole('heading', { name: 'Phones', level: 3 }) });
  await expect(card.getByRole('link', { name: 'Download app' })).toHaveAttribute('href', app.link);
  const row = card.getByRole('listitem').filter({ hasText: '(206) 555-0100' });
  const tag = row.getByText('Update', { exact: true });
  await expect(tag).toBeVisible();
  // Shown, not cut short with the row's details: the tag is what's drawn at its own middle. The
  // row comes into view, never the tag itself, which would scroll the details' clipped line too.
  await row.scrollIntoViewIfNeeded();
  const drawn = await tag.evaluate((element) => {
    const box = element.getBoundingClientRect();
    const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
    return hit !== null && element.contains(hit);
  });
  expect(drawn).toBe(true);

  await page.setViewportSize({ width: 390, height: 844 });
  await row.scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  for (const element of [card, row]) {
    const bounds = (await element.boundingBox())!;
    expect(bounds.x).toBeGreaterThanOrEqual(0);
    expect(bounds.x + bounds.width).toBeLessThanOrEqual(390);
  }
  const number = row.getByText('(206) 555-0100', { exact: true });
  const details = row.getByText(/^Dispatch phone at the north station, by the loading dock ·/);
  for (const text of [number, details]) {
    await expect(text).toBeInViewport();
    expect(await text.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
  }
  await expect(tag).toBeInViewport();
  await expect(row.getByText('Offline · 3 min', { exact: true })).toBeInViewport();
  await expect(row.getByText('No SIM', { exact: true })).toBeInViewport();
  const actions = row.getByLabel(
    'Actions for Dispatch phone at the north station, by the loading dock',
  );
  await expect(actions).toBeInViewport();
  await actions.click();
  await expect(row.getByRole('button', { name: 'Rename', exact: true })).toBeInViewport();
  await expect(row.getByRole('button', { name: 'Remove', exact: true })).toBeInViewport();
  await page.keyboard.press('Escape');

  await card.getByRole('button', { name: 'Add phone' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add phone' });
  const install = dialog.getByRole('region', { name: 'Install' });
  await expect(
    install.getByRole('img', { name: 'Download DSP Dispatch: Relay Dev' }),
  ).toHaveAttribute('src', app.picture);
  await expect(install).toContainText('DSP Dispatch: Relay Dev 1.4.0');
  await expect(
    dialog
      .getByRole('region', { name: 'Pair' })
      .getByRole('img', { name: /^Pairing code [A-HJ-NP-Z2-9]{8}$/ }),
  ).toBeVisible();
});

test('the banner of a phone down follows it on every page, with alerts off', async ({
  page,
  dispatch,
}) => {
  // Neither a chime nor a notification: nothing reads what waits for alerts.
  const owner = await dispatch.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const { sections } = await owner.read('/api/dsp/settings');
  const display = sections.find(
    (section: { id: string }) => section.id === 'communications.display',
  );
  const quiet = await owner.post('/api/dsp/settings/communications.display', {
    values: { sound: false, notifications: false },
    revision: display.revision,
  });
  expect(quiet.status, quiet.body).toBe(200);
  // The DSP's phone as the server answers it, sending or not.
  let cantSend = false;
  await page.route('**/api/dsp/communications/waiting', async (route) => {
    const response = await route.fetch();
    const phone = {
      id: 'phone_x',
      name: 'Dispatch',
      online: true,
      lastSeenAt: Date.now(),
      cantSend,
    };
    await route.fulfill({ response, json: { ...(await response.json()), phones: [phone] } });
  });
  await page.clock.install();
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  await expect(page.getByRole('navigation', { name: 'Settings', exact: true })).toBeVisible();
  const banner = page.getByText("Dispatch phone can't send texts");
  await expect(banner).toHaveCount(0);
  // It stops sending while Settings stays open, then sends again.
  cantSend = true;
  await page.clock.runFor(30_000);
  await expect(banner).toBeVisible();
  cantSend = false;
  await page.clock.runFor(30_000);
  await expect(banner).toHaveCount(0);
});
