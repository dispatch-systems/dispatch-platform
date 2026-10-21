import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import { connect, event, fromSlack, install } from '../../../slack/tests/support/slack.js';

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
  // to where it opened; their photo by their messages opens it too, its X closes it, back to that
  // photo, and a press elsewhere closes it.
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
  await profile.getByRole('button', { name: 'Close' }).click();
  await expect(profile).toHaveCount(0);
  await expect(opensProfile.last()).toBeFocused();
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

test('with two workspaces, the lists show the one chosen, still chosen later, and Contacts opens a conversation', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  expect((await connect(owner, 'T0FIXTURE1')).status).toBe(200);
  expect((await connect(owner, 'T0FIXTURE2')).status).toBe(200);
  // Jordan Ellis writes in Operations, Cameron Hayes in Drivers.
  const write = (team: string, user: string, text: string) =>
    fromSlack(
      dispatch,
      '/slack/events',
      event(team, {
        type: 'message',
        channel_type: 'im',
        channel: `D${user.slice(1)}`,
        user,
        text,
        ts: (Date.now() / 1000).toFixed(6),
      }),
    );
  expect((await write('T0FIXTURE1', 'U0FIXJORDAN', 'Running late')).status).toBe(200);
  expect((await write('T0FIXTURE2', 'U0FIXCAM', 'Can I swap Saturday?')).status).toBe(200);

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  const list = page.getByRole('region', { name: 'Conversations' });
  const jordan = list.getByRole('listitem').filter({ hasText: 'Jordan Ellis' });
  const cameron = list.getByRole('listitem').filter({ hasText: 'Cameron Hayes' });
  await expect(jordan).toBeVisible();
  await expect(cameron).toBeVisible();
  // Drivers alone, which stays chosen when the page opens again.
  await list.getByLabel('Workspace: All workspaces').click();
  await list.getByRole('button', { name: /^Drivers/ }).click();
  await expect(jordan).toHaveCount(0);
  await expect(cameron).toBeVisible();
  await page.reload();
  await expect(list.getByLabel('Workspace: Drivers')).toBeVisible();
  await expect(jordan).toHaveCount(0);

  // Contacts: Drivers' members, A to Z; one with no conversation yet starts one.
  await list.getByRole('tab', { name: 'Contacts' }).click();
  const contacts = page.getByRole('region', { name: 'Contacts' });
  await expect(contacts.getByRole('listitem')).toHaveText([
    /Cameron Hayes/,
    /Casey Rivera/,
    /Drew Sullivan/,
    /Jordan Ellis/,
  ]);
  await contacts.getByRole('button', { name: /^Casey Rivera/ }).click();
  const casey = page.getByRole('region', { name: 'Casey Rivera' });
  await expect(casey.getByLabel('Message')).toBeEnabled();

  // On a wide page, the profile is beside the conversation, and a press elsewhere leaves it.
  await page.setViewportSize({ width: 1920, height: 1080 });
  await casey
    .getByRole('button', { name: /^Casey Rivera/ })
    .first()
    .click();
  const profile = page.getByRole('dialog', { name: 'Casey Rivera' });
  await expect(profile).toBeVisible();
  await page.getByRole('heading', { name: 'Communications', level: 1 }).click();
  await expect(profile).toBeVisible();
});
