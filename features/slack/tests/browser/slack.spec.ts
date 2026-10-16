import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, and its owner holds every permission. Fixture
// mode's Slack sends the browser straight back from installing.

test('an owner adds Slack workspaces from New, each on a card under its name, using what Slack sent back once', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByRole('tab', { name: 'Connections', exact: true }).click();
  const workspaces = page
    .getByRole('article')
    .filter({ has: page.getByRole('term').filter({ hasText: 'Workspace' }) });
  await expect(workspaces).toHaveCount(0);
  const add = async (name = '') => {
    await page.getByLabel('New', { exact: true }).click();
    await page.getByRole('button', { name: 'Slack', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'New Slack connection' });
    await dialog.getByLabel('Name').fill(name);
    await dialog.getByRole('button', { name: 'Connect Slack' }).click();
  };
  await add('Dispatch Ops');
  // Back on Settings' Connections, the install finished under the name it was given, and Slack's
  // code left the address; the tab stays.
  const named = workspaces.filter({ has: page.getByRole('heading', { name: 'Dispatch Ops' }) });
  await expect(named).toContainText('Operations');
  await expect(named).toContainText('Connected');
  await expect(page).toHaveURL(/tab=connections/);
  expect(page.url()).not.toContain('slackCode');

  // Without a name, it goes by its name in Slack.
  await add();
  await expect(workspaces.getByRole('heading', { name: 'Drivers' })).toBeVisible();
  await named.getByRole('button', { name: 'Disconnect' }).click();
  await page
    .getByRole('dialog', { name: 'Disconnect Dispatch Ops?' })
    .getByRole('button', { name: 'Disconnect' })
    .click();
  await expect(workspaces).toHaveText([/Drivers/]);
});
