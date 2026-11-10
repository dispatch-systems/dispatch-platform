import { test, expect, login, openDsp } from '../../../core/shell/tests/support/fixtures.js';

test('credentials close before login finishes, errors stay on the card, and CAPTCHA opens after retry', async ({
  page,
}) => {
  let connection: Record<string, unknown> = {
    provider: 'paycom',
    enabled: true,
    status: 'ready',
    error: null,
    updatedAt: new Date().toISOString(),
    lastVerifiedAt: null,
    accountLabel: 'DEMO',
    checks: [],
  };
  let submissions = 0;
  let finishRequest: () => void = () => {};
  await page.route('**/api/dsp/connections', (route) => route.fulfill({ json: connection }));
  await page.route('**/api/dsp/connections/paycom', async (route) => {
    submissions++;
    const attempt = submissions;
    await new Promise<void>((resolve) => {
      finishRequest = resolve;
    });
    if (attempt === 1) {
      connection = { ...connection, status: 'error', error: 'provider_unavailable' };
      await route.fulfill({
        status: 503,
        json: { error: 'provider_unavailable', message: 'Paycom is temporarily unavailable.' },
      });
    } else {
      connection = {
        ...connection,
        status: 'needs_verification',
        error: null,
        verificationSessionId: 'run_' + 'b'.repeat(32),
      };
      await route.fulfill({ json: connection });
    }
  });
  await page.route('**/api/dsp/connections/paycom/screenshot?*', (route) =>
    route.fulfill({
      json: {
        sessionId: connection.verificationSessionId,
        image:
          'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j0xkAAAAASUVORK5CYII=',
      },
    }),
  );
  try {
    await login(page);
    await openDsp(page, 'Northline Logistics');
    // The platform's Settings is a page, and the DSP's a sidebar of them: the button is the
    // DSP's, there once the selected DSP's navigation is.
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
      .filter({ has: page.getByRole('heading', { name: 'Paycom', exact: true }) });
    const credentials = page.getByRole('dialog', { name: 'Enter your Paycom login', exact: true });
    for (const attempt of [1, 2]) {
      await card.getByRole('button', { name: 'Update credentials' }).click();
      // Connected by signing in, the dialog opens on that way: on to its login, past the
      // check of the security questions.
      for (const step of ['Connect Paycom', 'Check your security questions'])
        await page
          .getByRole('dialog', { name: step, exact: true })
          .getByRole('button', { name: 'Next' })
          .click();
      await expect(credentials.getByLabel('Username', { exact: true })).toHaveValue('');
      await expect(credentials.getByLabel('Password', { exact: true })).toHaveValue('');
      await credentials.getByLabel('Username', { exact: true }).fill('fixture-user');
      await credentials.getByLabel('Password', { exact: true }).fill('fixture-password');
      for (const number of [1, 2, 3, 4, 5])
        await credentials
          .getByLabel(`PIN #${number}`, { exact: true })
          .fill(`fixture-pin-${number}`);
      if (attempt === 1) {
        await credentials.getByLabel('PIN #5', { exact: true }).fill('fixture-pin-4');
        await credentials.getByRole('button', { name: 'Connect', exact: true }).click();
        await expect(credentials.getByRole('alert')).toContainText('five distinct security PINs');
        expect(submissions).toBe(0);
        await credentials.getByLabel('PIN #5', { exact: true }).fill('fixture-pin-5');
      }
      await credentials.getByRole('button', { name: 'Connect', exact: true }).click();
      await expect.poll(() => submissions).toBe(attempt);
      // The response is deliberately held open: closing must not wait for Paycom.
      await expect(credentials).toHaveCount(0);
      await expect(card.getByText('Signing in to Paycom…', { exact: true })).toBeVisible();
      await expect(card.getByRole('button', { name: 'Update credentials' })).toBeDisabled();
      await expect(card.getByRole('alert')).toHaveCount(0);
      finishRequest();
      if (attempt === 1) {
        await expect(card.getByRole('alert')).toHaveText('Paycom is temporarily unavailable.');
        await expect(credentials).toHaveCount(0);
        await expect(card.getByRole('button', { name: 'Update credentials' })).toBeEnabled();
      }
    }
    await expect(page.getByRole('dialog', { name: 'Complete Paycom verification' })).toBeVisible();
    await expect(credentials).toHaveCount(0);
  } finally {
    finishRequest();
  }
});

test("connecting through Paycom's API is led step by step, and the card says which check failed", async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Summit Delivery');
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
    .filter({ has: page.getByRole('heading', { name: 'Paycom', exact: true }) });
  const step = (name: string) => page.getByRole('dialog', { name, exact: true });
  const next = (name: string) => step(name).getByRole('button', { name: 'Next' }).click();
  /** Enters an SID and a Token on the last step, and connects. */
  const connect = async (token: string) => {
    const entry = step('Enter your SID and Token');
    await entry.getByLabel('SID', { exact: true }).fill('SID123');
    await entry.getByLabel('Token', { exact: true }).fill(token);
    await entry.getByRole('button', { name: 'Connect', exact: true }).click();
  };
  // The demo DSP signs in: the dialog opens on that way, and the API is the other.
  await card.getByRole('button', { name: 'Update credentials' }).click();
  await expect(step('Connect Paycom').getByRole('radio', { name: /Paycom sign-in/ })).toBeChecked();
  await step('Connect Paycom')
    .getByRole('radio', { name: /Paycom API/ })
    .check();
  await next('Connect Paycom');
  // The address Paycom has to allow, as the server names it.
  await expect(step('Ask for API access')).toContainText('203.0.113.10');
  await next('Ask for API access');
  await next('Turn these on in Paycom');
  // The fixture's Token for a client that left Punch History off.
  await connect('punch-history-off');
  await expect(card.getByRole('status')).toHaveText('Not ready');
  const checks = card.getByRole('listitem');
  await expect(checks.filter({ hasText: 'Punch History' })).toContainText(
    'Not turned on in Paycom',
  );
  await expect(checks.filter({ hasText: 'Employee Directory' }).getByLabel('Passed')).toBeVisible();
  // The card says why, and the top of the page doesn't say it again.
  await expect(page.getByRole('alert')).toHaveCount(0);
  // Setup steps opens on the step that puts it right, and says which function; checking
  // again from there closes it, and the card stands as Paycom still answers.
  await card.getByRole('button', { name: 'Setup steps' }).click();
  const functions = step('Turn these on in Paycom');
  await expect(functions.getByRole('listitem').filter({ hasText: 'Punch History' })).toContainText(
    'Not on yet',
  );
  await functions.getByRole('button', { name: 'Check again' }).click();
  await expect(functions).toHaveCount(0);
  await expect(card.getByRole('status')).toHaveText('Not ready');
  // Connected anew with a Token whose client has every function on: the list folds away.
  await card.getByRole('button', { name: 'Disconnect' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Disconnect' }).click();
  await card.getByRole('button', { name: 'Connect Paycom' }).click();
  await expect(step('Connect Paycom').getByRole('radio', { name: /Paycom API/ })).toBeChecked();
  await next('Connect Paycom');
  await next('Ask for API access');
  await next('Turn these on in Paycom');
  await connect('synthetic-token');
  await expect(card.getByRole('status')).toHaveText('Connected');
  await expect(card.getByRole('listitem')).toHaveCount(0);
  await expect(card.getByText('API', { exact: true })).toBeVisible();
});
