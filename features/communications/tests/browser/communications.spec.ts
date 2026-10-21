import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import { event, fromSlack, install } from '../../../slack/tests/support/slack.js';

// The seeded DSP has every feature switched on, and its owner holds every permission. Fixture
// mode's Slack sends the browser straight back from installing, to the first of its workspaces
// no DSP has: Operations, where Jordan Ellis and Morgan Lane are members.

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

test('an owner connects Slack from Communications, answers a member as they write, and writes to someone first', async ({
  page,
  dispatch,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  // Without Slack, connecting it takes the inbox's place, and there's no one to write to.
  await expect(
    page.getByRole('heading', { name: 'Employees message your DSP through Slack' }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'New message' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Connect Slack' }).click();
  // Slack sends the browser back here, which finishes connecting it.
  await expect(page.getByText('Slack connected')).toBeVisible();
  await expect(page).toHaveURL(/\/communications$/);

  // A member writes to the app: their conversation arrives waiting, counted in the sidebar.
  const write = (text: string) =>
    fromSlack(
      dispatch,
      '/slack/events',
      event('T0FIXTURE1', {
        type: 'message',
        channel_type: 'im',
        channel: 'D0FIXJORDAN',
        user: 'U0FIXJORDAN',
        text,
        ts: (Date.now() / 1000).toFixed(6),
      }),
    );
  expect((await write('Running late')).status).toBe(200);
  const list = page.getByRole('region', { name: 'Conversations' });
  const jordan = list.getByRole('listitem').filter({ hasText: 'Jordan Ellis' });
  await expect(jordan).toContainText('Running late');
  await expect(
    page.getByRole('navigation', { name: 'Primary navigation' }).getByLabel('1 waiting'),
  ).toBeVisible();
  // Beside the list, the first conversation is open: answered, it waits no more.
  const conversation = page.getByRole('region', { name: 'Jordan Ellis' });
  await conversation.getByLabel('Message').fill('On my way');
  await conversation.getByLabel('Message').press('Enter');
  await expect(conversation.getByText('On my way')).toBeVisible();
  await expect(list.getByRole('heading', { name: 'Responded' })).toBeVisible();
  await expect(list.getByRole('heading', { name: /^Waiting/ })).toHaveCount(0);

  // Who it's with, above, opens their profile over the conversation, and Escape closes it, back
  // to where it opened; their photo by their messages opens it too, and a press elsewhere
  // closes it.
  const opensProfile = conversation.getByRole('button', { name: /^Jordan Ellis/ });
  await opensProfile.first().click();
  const profile = page.getByRole('dialog', { name: 'Jordan Ellis' });
  await expect(profile.getByRole('button', { name: 'Close' })).toBeFocused();
  await expect(profile).toContainText('jordan.ellis@example.com');
  await expect(profile).toContainText('Operations');
  await page.keyboard.press('Escape');
  await expect(profile).toHaveCount(0);
  await expect(opensProfile.first()).toBeFocused();
  await opensProfile.last().click();
  await expect(profile).toBeVisible();
  await page.getByRole('heading', { name: 'Communications', level: 1 }).click();
  await expect(profile).toHaveCount(0);

  // Written to again, it waits, until the check on its row marks it done without a reply.
  expect((await write('Thanks')).status).toBe(200);
  await expect(list.getByRole('heading', { name: /^Waiting/ })).toBeVisible();
  await jordan.hover();
  await jordan.getByRole('button', { name: 'Done' }).click();
  await expect(list.getByRole('heading', { name: /^Waiting/ })).toHaveCount(0);

  // New message finds someone by a misspelt name, and their conversation stays at the top of
  // the list until it holds a message.
  await page.getByRole('button', { name: 'New message' }).click();
  const dialog = page.getByRole('dialog', { name: 'New message' });
  await dialog.getByLabel('Name').fill('morgn lane');
  await dialog.getByRole('option', { name: /Morgan Lane/ }).click();
  const morgan = list.getByRole('listitem').filter({ hasText: 'Morgan Lane' });
  await expect(morgan).toContainText('New message');
  const first = page.getByRole('region', { name: 'Morgan Lane' });
  await first.getByLabel('Message').fill('Welcome aboard');
  await first.getByRole('button', { name: 'Send' }).click();
  await expect(morgan).toContainText('Welcome aboard');

  // Copy chat link puts the address of the app's Messages tab on the clipboard.
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], {
    origin: new URL(page.url()).origin,
  });
  await page.getByRole('button', { name: 'Copy chat link' }).click();
  await expect(page.getByText('Chat link copied')).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
    'https://slack.com/app_redirect?app=A0FIXTURE&team=T0FIXTURE1',
  );
});

test('Slack needing an update covers Communications alone, with no way to close it, until it is updated', async ({
  page,
  dispatch,
}) => {
  // Its admin approved the app before it answered direct messages.
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  const started = await owner.post('/api/dsp/slack/connect', { label: '' });
  const { state } = install(started.value.url);
  const earlier = 'fixture:T0FIXTURE1:team:read,users:read,users:read.email';
  expect((await owner.post('/api/dsp/slack/connect/finish', { state, code: earlier })).status).toBe(
    200,
  );

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  const update = page.getByRole('dialog', { name: 'Update Slack' });
  await expect(update.getByRole('listitem')).toHaveText(/Operations/);
  await page.keyboard.press('Escape');
  await expect(update).toBeVisible();
  // The sidebar still goes elsewhere.
  await page.getByRole('link', { name: 'Team & Roles' }).click();
  await expect(page).toHaveURL(/\/team/);
  await expect(update).toHaveCount(0);
  await page.getByRole('link', { name: 'Communications' }).click();
  // Updated from here, Slack comes back here, and it's gone.
  await update.getByRole('button', { name: 'Update' }).click();
  await expect(page.getByText('Slack connected')).toBeVisible();
  await expect(page).toHaveURL(/\/communications$/);
  await expect(update).toHaveCount(0);
});
