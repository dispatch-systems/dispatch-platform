import { test, expect, login } from '../../../../core/shell/tests/support/fixtures.js';

test('a member opens their DSP on its home page, blank until they add a widget, and still being built in a window as narrow as a drawer', async ({
  page,
}) => {
  await login(page, 'member@dispatch.test');
  await expect(page.getByRole('heading', { name: 'Home', exact: true, level: 1 })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Add widget', exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Home Page', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await page.setViewportSize({ width: 980, height: 900 });
  await expect(
    page.getByRole('heading', { name: 'Currently under development', exact: true }),
  ).toBeVisible();
  await expect(page.getByText('We’re building your DSP home page.', { exact: true })).toBeVisible();
});
