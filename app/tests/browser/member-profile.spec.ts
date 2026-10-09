import type { Page } from '@playwright/test';
import {
  test,
  expect,
  demo,
  fromPage,
  linkIn,
  login,
  openDsp,
} from '../../../core/shell/tests/support/fixtures.js';
import { capturedMail } from '../../../core/shell/tests/support/mail-support.js';
import type { fixture } from '../../../core/shell/tests/support/support.js';
import { fits } from '../../../core/accounts/tests/support/member-profile.js';

// A member's profile, from an invitation the team sends: core's page, after Team's invite.

async function inviteMember(page: Page, root: string) {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Team & Roles', exact: true }).click();
  await page.getByRole('button', { name: 'Invite member', exact: true }).click();
  const sheet = page.getByRole('dialog', { name: 'Invite member' });
  await sheet.getByLabel('Email address').fill('new-member@dispatch.test');
  await sheet.getByLabel('Role').selectOption({ label: 'Member' });
  await sheet.getByRole('button', { name: 'Send invitation' }).click();
  await expect(
    page.getByText('Invitation email queued for new-member@dispatch.test', { exact: true }),
  ).toBeVisible();
  const mail = await capturedMail(root, 'new-member@dispatch.test');
  // Leave the owner's dashboard before its session is cleared, so the invite opens in a
  // fresh page as an invitee's would. Otherwise the dashboard's own refreshes after the
  // invite are refused without the session, and signing the owner out can race the link.
  await page.goto('about:blank');
  // The link opens at Northline's own address.
  return linkIn(mail.text);
}

test('a DSP member invite creates a profile in its own responsive map screen', async ({
  page,
  dispatch,
  context,
}) => {
  const url = await inviteMember(page, dispatch.root);
  await context.clearCookies();
  const errors: string[] = [];
  const logins: string[] = [];
  page.on('request', (request) => {
    if (request.url().endsWith('/api/auth/login')) logins.push(request.url());
  });
  const assets: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('request', (request) => assets.push(request.url()));
  await page.goto(url);
  await expect(page).toHaveURL(/#invite$/);
  expect(page.url()).not.toContain('token=');
  await expect(page.getByRole('heading', { name: 'Create your profile' })).toBeVisible();
  await expect(page.locator('.auth-layout, .onboarding-page')).toHaveCount(0);
  await expect(page.getByLabel('Email address')).toHaveValue('new-member@dispatch.test');
  await expect(page.getByLabel('Email address')).toHaveAttribute('readonly', '');
  await expect(page.getByLabel('DSP name', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('list', { name: 'Onboarding progress' })).toHaveCount(0);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByLabel('First name', { exact: true }).fill('New');
  await page.getByLabel('Last name', { exact: true }).fill('Member');
  await page.getByLabel('Password', { exact: true }).fill(demo.password);
  await page.getByLabel('Confirm password', { exact: true }).fill('Different-password');
  await page.getByRole('button', { name: 'Show password', exact: true }).click();
  await expect(page.getByLabel('Password', { exact: true })).toHaveAttribute('type', 'text');
  await page.getByRole('button', { name: 'Hide password', exact: true }).click();
  await page.getByRole('button', { name: 'Create profile', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('The passwords must match.');
  await fits(page);
  await page.getByLabel('Confirm password', { exact: true }).fill(demo.password);
  await page.getByRole('button', { name: 'Create profile', exact: true }).click();
  await expect(page).toHaveURL(/#signin$/);
  await expect(page.getByLabel('Email address')).toHaveValue('new-member@dispatch.test');
  await expect(page.getByLabel('Password', { exact: true })).toBeFocused();
  expect(logins).toEqual([]);
  expect((await fromPage(page, '/api/session')).status).toBe(401);
  await page.getByLabel('Password', { exact: true }).fill(demo.password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'Currently under development', exact: true }),
  ).toBeVisible();
  expect(new URL(page.url()).hostname).toBe(`${demo.memberDsp}.localhost`);
  await expect(page.locator('.member-profile-page')).toHaveCount(0);
  const session = (await fromPage(page, '/api/session')).value;
  expect(session.user.email).toBe('new-member@dispatch.test');
  expect(session.dsps).toHaveLength(1);
  expect(session.dsps[0].name).toBe('Northline Logistics');
  expect(assets.filter((url) => /onboarding-map|login-van|renderer-.*\.js/.test(url))).toEqual([]);
  expect(errors).toEqual([]);
});

// Exercise the full owner UI above; completion variants use the same real invitation API.
async function apiInvitation(dispatch: Awaited<ReturnType<typeof fixture>>) {
  const owner = await dispatch.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  const view = await owner.select(dsp.id);
  const role = view.roles.find((r: { name: string }) => r.name === 'Member');
  const result = await owner.post('/api/dsp/members/invite', {
    email: 'new-member@dispatch.test',
    role: role.id,
  });
  expect(result.status).toBe(200);
  return linkIn((await capturedMail(dispatch.root, 'new-member@dispatch.test')).text);
}

test('an accepted invitation says so and hands its email to sign-in', async ({
  page,
  dispatch,
  context,
}) => {
  const url = await apiInvitation(dispatch);
  await context.clearCookies();
  const token = url.split('token=')[1]!;
  const accepted = await dispatch.request(
    `/api/invitations/${token}/accept`,
    { firstName: 'Jamie', lastName: 'Morgan', password: demo.password },
    dispatch.at(demo.memberDsp),
  );
  expect(accepted.status).toBe(200);
  await page.goto(url);
  await expect(page.getByRole('heading', { name: 'Already accepted' })).toBeVisible();
  await expect(page.locator('.member-profile-invitation')).toHaveText(
    'NLNorthline LogisticsMember',
  );
  await expect(
    page.getByText(
      'You joined Northline Logistics with this invitation. Sign in with your Dispatch password.',
    ),
  ).toBeVisible();
  await expect(page.locator('.member-profile-map')).toHaveAttribute('data-route', 'delivered');
  await expect(page.getByLabel('Password', { exact: true })).toHaveCount(0);
  for (const [width, height] of [
    [1440, 1000],
    [1024, 600],
    [390, 844],
    [568, 320],
  ]) {
    await page.setViewportSize({ width: width!, height: height! });
    await fits(page);
  }
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page).toHaveURL(/#signin$/);
  await expect(page.getByLabel('Email address')).toHaveValue('new-member@dispatch.test');
  await expect(page.getByLabel('Password', { exact: true })).toBeFocused();
  expect((await fromPage(page, '/api/session')).status).toBe(401);
});

async function fillMemberProfile(page: Page) {
  await page.getByLabel('First name', { exact: true }).fill('Jamie');
  await page.getByLabel('Last name', { exact: true }).fill('Morgan');
  await page.getByLabel('Password', { exact: true }).fill(demo.password);
  await page.getByLabel('Confirm password', { exact: true }).fill(demo.password);
}

for (const variant of ['phone', 'reduced motion', 'unavailable artwork'] as const) {
  test(`member completion goes directly to sign-in with ${variant}`, async ({
    page,
    dispatch,
    context,
  }) => {
    const url = await apiInvitation(dispatch);
    await context.clearCookies();
    await page.setViewportSize(
      variant === 'phone' ? { width: 390, height: 844 } : { width: 1280, height: 800 },
    );
    await page.emulateMedia({
      reducedMotion: variant === 'reduced motion' ? 'reduce' : 'no-preference',
    });
    let failedArtwork = 0;
    if (variant === 'unavailable artwork')
      await page.route('**/MemberProfileCompletion-*.js', (route) => {
        failedArtwork++;
        return route.abort();
      });
    const assets: string[] = [];
    page.on('request', (request) => assets.push(request.url()));
    await page.goto(url);
    await fillMemberProfile(page);
    // A failed save must leave the form usable, without a success animation or navigation.
    await page.route('**/api/invitations/*/accept', (route) => route.abort(), { times: 1 });
    await page.getByRole('button', { name: 'Create profile', exact: true }).click();
    await expect(page.getByRole('alert')).toBeVisible();
    await expect(page.locator('.member-completion')).toHaveCount(0);
    await expect(page).toHaveURL(/#invite$/);
    await page.getByRole('button', { name: 'Create profile', exact: true }).click();
    await expect(page).toHaveURL(/#signin$/);
    await expect(page.locator('.auth-panel .notice')).toHaveCount(0);
    await expect(page.getByLabel('Email address')).toHaveValue('new-member@dispatch.test');
    await expect(page.locator('.auth-layout')).toHaveAttribute('data-enter', 'false');
    expect((await fromPage(page, '/api/session')).status).toBe(401);
    if (variant !== 'unavailable artwork')
      expect(assets.filter((url) => /MemberProfileCompletion-/.test(url))).toEqual([]);
    else expect(failedArtwork).toBe(1);
    if (variant === 'phone')
      expect(
        assets.filter((url) => /member-profile-map|login-van|renderer-.*\.js/.test(url)),
      ).toEqual([]);
  });
}

test('resizing to a phone during member completion finishes the handoff immediately', async ({
  page,
  dispatch,
  context,
}) => {
  const url = await apiInvitation(dispatch);
  await context.clearCookies();
  await page.goto(url);
  await fillMemberProfile(page);
  await page.getByRole('button', { name: 'Create profile', exact: true }).click();
  await expect(page.getByRole('status', { name: 'Profile created' })).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page).toHaveURL(/#signin$/);
  await expect(page.locator('.auth-layout')).toHaveAttribute('data-enter', 'false');
  await expect(page.locator('.member-completion')).toHaveCount(0);
  expect((await fromPage(page, '/api/session')).status).toBe(401);
});
