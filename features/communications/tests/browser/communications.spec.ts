import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, and its owner holds every permission. Fixture
// mode's Slack sends the browser straight back from installing.

test('Communications is on from the start in a workspace just connected', async ({ page }) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('navigation', { name: 'Settings', exact: true });
  await settings.getByRole('link', { name: 'Connections', exact: true }).click();
  await page.getByLabel('New', { exact: true }).click();
  await page.getByRole('button', { name: 'Slack', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'New Slack connection' })
    .getByRole('button', { name: 'Connect Slack' })
    .click();
  const workspace = page
    .getByRole('article')
    .filter({ has: page.getByRole('heading', { name: 'Operations' }) });
  await workspace.getByRole('link', { name: 'Settings' }).click();
  await page
    .getByRole('navigation', { name: 'Sections' })
    .getByRole('button', { name: 'Integrations', exact: true })
    .click();
  // Its card, under the workspace's Integrations, on.
  await expect(page.getByRole('switch', { name: 'Communications', exact: true })).toBeChecked();
});
