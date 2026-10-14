import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, and its owner holds every permission. Fixture
// mode's Slack sends the browser straight back from installing.

test('an owner connects Slack from its card, which uses what Slack sent back once', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByRole('tab', { name: 'Connections', exact: true }).click();
  const slack = page
    .getByRole('article')
    .filter({ has: page.getByRole('heading', { name: 'Slack', exact: true }) });
  await slack.getByRole('button', { name: 'Connect Slack' }).click();
  // Back on Settings' Connections, the card finished the install and took Slack's code out of
  // the address; the tab stays.
  await expect(slack.getByRole('listitem').filter({ hasText: 'Operations' })).toContainText(
    'Connected',
  );
  await expect(page).toHaveURL(/tab=connections/);
  expect(page.url()).not.toContain('slackCode');

  await slack.getByRole('button', { name: 'Add workspace' }).click();
  await expect(slack.getByRole('listitem')).toHaveCount(2);
  await slack
    .getByRole('listitem')
    .filter({ hasText: 'Operations' })
    .getByRole('button', { name: 'Disconnect' })
    .click();
  await page
    .getByRole('dialog', { name: 'Disconnect Operations?' })
    .getByRole('button', { name: 'Disconnect' })
    .click();
  await expect(slack.getByRole('listitem')).toHaveText([/Drivers/]);
});
