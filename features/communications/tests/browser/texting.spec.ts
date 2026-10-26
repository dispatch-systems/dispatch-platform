import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import { connect, event, fromSlack } from '../../../slack/tests/support/slack.js';

// The seeded DSP texts through its demo's stand-in phone, (206) 555-0100, which received three
// texts and delivers what it's given at once; it has no Slack until a test connects it.

test('with its phone and no Slack, Communications is Messaging: a number saved as a contact, answered by text, blocked into Contacts and unblocked', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  const list = page.getByRole('region', { name: 'Conversations' });
  const stranger = list.getByRole('listitem').filter({ hasText: '(425) 555-0199' });
  await expect(stranger).toContainText('Hi is this dispatch? I start Monday');
  // Messaging alone: nothing to switch to, and nothing of Slack.
  await expect(page.getByRole('tablist', { name: 'Sections' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Connect Slack' })).toHaveCount(0);
  await stranger.getByRole('button').first().click();
  await page
    .getByRole('region', { name: '(425) 555-0199' })
    .getByRole('button', { name: 'Save contact' })
    .click();
  await page.getByLabel('Name', { exact: true }).fill('Jamie Ortiz');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(list.getByRole('listitem').filter({ hasText: 'Jamie Ortiz' })).toBeVisible();

  // Answered by text, it goes from the DSP's phone, which delivers it.
  const jamie = page.getByRole('region', { name: 'Jamie Ortiz' });
  await jamie.getByLabel('Message').fill('See you Monday at 7');
  await jamie.getByLabel('Message').press('Enter');
  await expect(jamie.getByText('Delivered')).toBeVisible();

  // Blocked, it leaves Messages for Contacts' Blocked, until it's unblocked.
  await jamie.getByLabel('More').click();
  await jamie.getByRole('button', { name: 'Block', exact: true }).click();
  await expect(list.getByRole('listitem').filter({ hasText: 'Jamie Ortiz' })).toHaveCount(0);
  await page.getByRole('tab', { name: 'Contacts' }).click();
  const contacts = page.getByRole('region', { name: 'Contacts' });
  await contacts
    .getByRole('listitem')
    .filter({ hasText: 'Jamie Ortiz' })
    .getByRole('button', { name: 'Unblock' })
    .click();
  await expect(contacts.getByRole('button', { name: 'Unblock' })).toHaveCount(0);
  await expect(contacts.getByRole('listitem').filter({ hasText: 'Jamie Ortiz' })).toContainText(
    '(425) 555-0199',
  );
});

test('with Slack too, the sections switch, each with how many wait, stay as chosen, and a link to a text opens Messaging', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  expect((await connect(owner, 'T0FIXTURE1')).status).toBe(200);
  const written = await fromSlack(
    dispatch,
    '/slack/events',
    event('T0FIXTURE1', {
      type: 'message',
      channel_type: 'im',
      channel: 'D0FIXJORDAN',
      user: 'U0FIXJORDAN',
      text: 'Running late',
      ts: (Date.now() / 1000).toFixed(6),
    }),
  );
  expect(written.status).toBe(200);

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  const sections = page.getByRole('tablist', { name: 'Sections' });
  const messaging = sections.getByRole('tab', { name: /Messaging/ });
  const slack = sections.getByRole('tab', { name: /Slack/ });
  await expect(messaging).toHaveText('Messaging3');
  await expect(slack).toHaveText('Slack1');
  const list = page.getByRole('region', { name: 'Conversations' });
  await expect(list).toContainText('(425) 555-0199');
  await slack.click();
  await expect(list.getByRole('listitem').filter({ hasText: 'Jordan Ellis' })).toBeVisible();
  await expect(list).not.toContainText('(425) 555-0199');
  await page.reload();
  await expect(slack).toHaveAttribute('aria-selected', 'true');

  // A link to a text, as a new one's notification is, opens it in Messaging.
  const { conversations } = await owner.read('/api/dsp/communications');
  const text = (conversations as { id: string; number: string | null }[]).find(
    (each) => each.number === '+14255550199',
  )!;
  await page.goto(`/#dsp/${north.id}/communications?open=${text.id}`);
  await expect(page.getByRole('region', { name: '(425) 555-0199' })).toBeVisible();
  await expect(messaging).toHaveAttribute('aria-selected', 'true');
});
