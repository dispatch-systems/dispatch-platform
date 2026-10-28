import type { Route } from '@playwright/test';
import {
  test,
  expect,
  demo,
  login,
  openDsp,
} from '../../../../core/shell/tests/support/fixtures.js';
import type { PhonePairing } from '../../api/index.js';

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
