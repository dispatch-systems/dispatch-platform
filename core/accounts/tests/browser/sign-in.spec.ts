import type { Page } from '@playwright/test';
import { test, expect, demo, login, signIn } from '../../../shell/tests/support/fixtures.js';
import { capturedMail } from '../../../shell/tests/support/mail-support.js';

test.use({ launchOptions: { args: ['--enable-unsafe-swiftshader'] } });

async function signOut(page: Page) {
  await page.locator('.account-menu summary').click();
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Sign in', exact: true })).toBeVisible();
}

for (const preference of ['Light', 'Dark', 'System'] as const) {
  test(`sign-in remembers ${preference} after sign-out and reload`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: preference === 'Dark' ? 'light' : 'dark' });
    await login(page);
    await page.getByRole('link', { name: 'Settings', exact: true }).click();
    await page.getByRole('tab', { name: 'Theme', exact: true }).click();
    await page.getByRole('radio', { name: preference, exact: true }).check();
    // Existing account preferences must also be remembered without being selected again.
    await page.evaluate(() => localStorage.removeItem('dispatch-appearance:signed-out'));
    await page.reload();
    await signOut(page);
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: 900 });
      await page.reload();
      for (const colorScheme of ['light', 'dark'] as const) {
        await page.emulateMedia({ colorScheme });
        const expected = preference === 'System' ? colorScheme : preference.toLowerCase();
        await expect(page.locator('html')).toHaveAttribute('data-theme', expected);
        await expect(page.locator('body')).toHaveCSS(
          'background-color',
          expected === 'light' ? 'rgb(255, 255, 255)' : 'rgb(17, 21, 29)',
        );
        await expect(page.getByLabel('Email address')).toBeVisible();
      }
    }
  });
}

test('remembering sign-in appearance keeps different accounts independent', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await login(page);
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByRole('tab', { name: 'Theme', exact: true }).click();
  await page.getByRole('radio', { name: 'Dark', exact: true }).check();
  await signOut(page);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await signIn(page, demo.member);
  await expect(page.locator('.account-menu')).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await signOut(page);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await signIn(page);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('remember me uses seven days and unchecked sign-in keeps eight hours', async ({
  page,
  context,
}) => {
  await page.goto('/');
  const remember = page.getByRole('checkbox', { name: 'Remember Me', exact: true });
  await expect(remember).not.toBeChecked();
  await remember.check();
  await signIn(page);
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  let cookie = (await context.cookies()).find((cookie) =>
    cookie.name.includes('dispatch_session'),
  )!;
  expect(cookie.httpOnly).toBe(true);
  expect(cookie.sameSite).toBe('Strict');
  expect(cookie.expires - Date.now() / 1000).toBeGreaterThan(604_700);
  expect(cookie.expires - Date.now() / 1000).toBeLessThanOrEqual(604_800);
  await page.reload();
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  await context.clearCookies();
  await page.goto('/');
  await expect(remember).not.toBeChecked();
  await signIn(page);
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  cookie = (await context.cookies()).find((cookie) => cookie.name.includes('dispatch_session'))!;
  expect(cookie.expires - Date.now() / 1000).toBeGreaterThan(28_700);
  expect(cookie.expires - Date.now() / 1000).toBeLessThanOrEqual(28_800);
});

test('mobile loads only the form; reset and password reveal still work', async ({
  page,
  dispatch,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const assets: string[] = [];
  page.on('request', (request) => assets.push(request.url()));
  await page.goto('/');
  for (const colorScheme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme });
    await expect(page.locator('html')).toHaveAttribute('data-theme', colorScheme);
    await expect(page.getByRole('heading', { name: 'Sign in', exact: true })).toBeVisible();
    await expect(page.locator('.login-art')).toBeHidden();
    await expect(page.locator('.login-van')).toHaveCount(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  }
  await page.getByLabel('Email address').fill(demo.email);
  await page.getByLabel('Password', { exact: true }).fill('wrong-password');
  await page.getByRole('button', { name: 'Show password', exact: true }).click();
  await expect(page.getByLabel('Password', { exact: true })).toHaveAttribute('type', 'text');
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page.getByRole('alert')).toBeVisible();
  await page.getByRole('button', { name: 'Forgot password?' }).click();
  await expect(page.getByRole('checkbox')).toHaveCount(0);
  await page.getByRole('button', { name: 'Send reset link' }).click();
  await expect(page.getByRole('status')).toContainText('reset link has been requested');
  const mail = await capturedMail(dispatch.root, demo.email);
  const token = /token=([A-Za-z0-9_-]{43})/.exec(mail.text)![1];
  await page.goto(`/#reset?token=${token}`);
  await expect(page).toHaveURL(/#reset$/);
  expect(page.url()).not.toContain(token);
  await page.getByLabel('Password', { exact: true }).fill('New-login-password-2026!');
  await page.getByLabel('Confirm password', { exact: true }).fill('New-login-password-2026!');
  await page.getByRole('button', { name: 'Update password' }).click();
  await expect(page.getByRole('heading', { name: 'Sign in', exact: true })).toBeVisible();
  await page.getByLabel('Email address').fill(demo.email);
  await page.getByLabel('Password', { exact: true }).fill('New-login-password-2026!');
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  expect(assets.filter((url) => /login-van|renderer-.*\.js|\/van\/|three/.test(url))).toEqual([]);
});

test.describe('desktop animation lifecycle', () => {
  test.use({ signInAnimation: true });
  test('graphics failure leaves a working sign-in form and a static van', async ({ page }) => {
    let failedModels = 0;
    await page.route('**/*login-van*.glb', (route) => {
      failedModels++;
      return route.abort();
    });
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.goto('/');
    // Prove that the real renderer reached the failing model request.
    await expect.poll(() => failedModels).toBe(1);
    await expect(page.locator('.login-van img')).toBeVisible();
    await signIn(page);
    await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  });
});
