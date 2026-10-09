import {
  test,
  expect,
  demo,
  dspAddress,
  fromPage,
  linkIn,
  login,
  signIn,
} from '../../../shell/tests/support/fixtures.js';
import { capturedMail } from '../../../shell/tests/support/mail-support.js';
import { createHash } from 'node:crypto';

// This flow performs additional sign-ins; like every browser test it owns its server,
// so its accounts and throttles stay isolated.
test('an address another DSP already has gets a login of its own at a newly invited DSP', async ({
  page,
  dispatch,
}) => {
  await login(page);
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  const origin = new URL(page.url()).origin;
  const session = await (await page.request.get(`${origin}/api/session`)).json();
  const created = await page.request.post(`${origin}/api/platform/dsps`, {
    headers: { Origin: origin, 'X-CSRF-Token': session.csrf },
    data: { name: 'Previous invitation DSP', ownerEmail: 'existing-invite@dispatch.test' },
  });
  expect(created.status()).toBe(201);
  const previous = await capturedMail(dispatch.root, 'existing-invite@dispatch.test');
  const raw = /token=([A-Za-z0-9_-]{43})/.exec(previous.text)![1];
  // That DSP has no short code yet, so its invitation opens at the invite page.
  const accepted = await dispatch.request(
    `/api/invitations/${raw}/accept`,
    { firstName: 'Existing', lastName: 'Member', password: demo.password },
    dispatch.at('invite'),
  );
  expect(accepted.status).toBe(200);
  // The prior membership predates this onboarding flow. Expire its recipient cooldown
  // without relaxing the production limit or waiting a minute in the browser test.
  dispatch.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare('UPDATE throttle SET reset_at=? WHERE key=?')
      .run(
        Date.now() - 1,
        createHash('sha256').update('mail:cooldown:existing-invite@dispatch.test').digest('hex'),
      ),
  );
  await page.getByRole('button', { name: 'Create new DSP', exact: true }).click();
  await page.getByLabel('Owner email').fill('existing-invite@dispatch.test');
  await page.getByRole('dialog').getByRole('button', { name: 'Create DSP', exact: true }).click();
  await expect(
    page.getByText('Invitation email queued for existing-invite@dispatch.test', { exact: true }),
  ).toBeVisible();
  const message = await capturedMail(dispatch.root, 'existing-invite@dispatch.test', previous.text);
  const nids = dspAddress(page, 'nids');
  await page.goto('about:blank');
  await page.setContent(message.html);
  await page.getByRole('link', { name: 'Start DSP onboarding', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Set up your DSP', exact: true })).toBeVisible();
  expect(new URL(page.url()).hostname).toBe('invite.localhost');
  await page.getByLabel('DSP name', { exact: true }).fill('New invited DSP');
  await page.getByLabel('Short code', { exact: true }).fill('NIDS');
  await expect(page.getByText('Your dashboard will be at nids.localhost')).toBeVisible();
  await page.getByLabel('Station code', { exact: true }).fill('TST1');
  await page.getByRole('button', { name: 'Continue to profile', exact: true }).click();
  // The new DSP keeps a login of its own, with a password of its own.
  const separate = 'A-separate-password-1!';
  await page.getByLabel('First name', { exact: true }).fill('Existing');
  await page.getByLabel('Last name', { exact: true }).fill('Member');
  await page.getByLabel('Password', { exact: true }).fill(separate);
  await page.getByLabel('Confirm password', { exact: true }).fill(separate);
  await page.getByRole('button', { name: 'Finish setup', exact: true }).click();
  // Its owner signs in at the address its short code gave it.
  await expect(page).toHaveURL(`${nids}#signin`);
  await page.getByLabel('Email address').fill('existing-invite@dispatch.test');
  await page.getByLabel('Password', { exact: true }).fill(separate);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'Currently under development', exact: true }),
  ).toBeVisible();
  const current = (await fromPage(page, '/api/session')).value;
  expect(current.dsps.map((dsp: { name: string }) => dsp.name)).toEqual(['New invited DSP']);
  expect(current.dsps[0].profile).toMatchObject({
    abbreviation: 'NIDS',
    stationCode: 'TST1',
    setupRequired: false,
  });
  expect(new URL(page.url()).origin).toBe(new URL(nids).origin);
});

// An owner who joined a DSP the platform owner already gave a short code is asked for its other
// details on opening it at its address, and they are saved where the owner that saves them says.
test('an owner whose DSP has no details yet sets them up on opening it', async ({
  page,
  dispatch,
}) => {
  await login(page);
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  const origin = new URL(page.url()).origin;
  const session = await (await page.request.get(`${origin}/api/session`)).json();
  const headers = { Origin: origin, 'X-CSRF-Token': session.csrf };
  const created = await page.request.post(`${origin}/api/platform/dsps`, {
    headers,
    // Made without a name, it waits for its owner to set it up.
    data: { ownerEmail: 'resumed-owner@dispatch.test' },
  });
  expect(created.status()).toBe(201);
  const id = (await created.json()).dsp.id;
  const coded = await page.request.post(`${origin}/api/platform/dsps/${id}/code`, {
    headers,
    data: { code: 'RDSP' },
  });
  expect(coded.status()).toBe(200);
  const mail = await capturedMail(dispatch.root, 'resumed-owner@dispatch.test');
  const raw = /token=([A-Za-z0-9_-]{43})/.exec(linkIn(mail.text))![1];
  // With its address already given, its owner may join first and set it up there.
  const accepted = await dispatch.request(
    `/api/invitations/${raw}/accept`,
    { firstName: 'Resumed', lastName: 'Owner', password: demo.password },
    dispatch.at('rdsp'),
  );
  expect(accepted.status).toBe(200);
  await page.context().clearCookies();
  await page.goto(dspAddress(page, 'rdsp'));
  await signIn(page, 'resumed-owner@dispatch.test');
  await expect(page.getByRole('heading', { name: 'Set up your DSP', exact: true })).toBeVisible();
  // Its short code is set, and stays.
  await expect(page.getByLabel('Short code', { exact: true })).toHaveValue('RDSP');
  await expect(page.getByLabel('Short code', { exact: true })).toHaveAttribute('readonly', '');
  await page.getByLabel('DSP name', { exact: true }).fill('Resumed DSP');
  await page.getByLabel('Station code', { exact: true }).fill('TST2');
  await page.getByRole('button', { name: 'Save DSP details', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'Currently under development', exact: true }),
  ).toBeVisible();
  const current = (await fromPage(page, '/api/session')).value;
  expect(current.dsps[0].profile).toMatchObject({
    abbreviation: 'RDSP',
    stationCode: 'TST2',
    setupRequired: false,
  });
});
