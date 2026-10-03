import { test, expect, login } from '../../../../core/shell/tests/support/fixtures.js';

test('a member opens their DSP on its home page', async ({ page }) => {
  await login(page, 'member@dispatch.test');
  await expect(
    page.getByRole('heading', { name: 'Currently under development', exact: true }),
  ).toBeVisible();
  await expect(page.getByText('We’re building your DSP home page.', { exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Home Page', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
});
