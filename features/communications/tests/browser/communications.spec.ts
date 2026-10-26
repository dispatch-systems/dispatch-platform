import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import { connect, event, fromSlack, install } from '../../../slack/tests/support/slack.js';

// The seeded DSP has every feature switched on, and its owner holds every permission. Fixture
// mode's Slack sends the browser straight back from installing, to the first of its workspaces
// no DSP has: Operations, where Jordan Ellis and Morgan Lane are members. These are Slack's, so
// each starts with Northline not texting, as its demo phone would have it (texting.spec.ts).
test.beforeEach(async ({ dispatch }) => {
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  const off = await owner.post(`/api/platform/dsps/${north.id}/features`, {
    feature: 'phones',
    enabled: false,
  });
  expect(off.status, off.body).toBe(200);
});

test('Communications is on from the start in a workspace just connected', async ({ page }) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
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
  // photo, and a press elsewhere closes it. Closed, it slides away and is gone: looked for hidden
  // too, as it is while it slides.
  const opensProfile = conversation.getByRole('button', { name: /^Jordan Ellis/ });
  await opensProfile.first().click();
  const profile = page.getByRole('dialog', { name: 'Jordan Ellis', includeHidden: true });
  await expect(profile.getByRole('button', { name: 'Close' })).toBeFocused();
  await expect(profile).toContainText('jordan.ellis@example.com');
  await expect(profile).toContainText('Operations');
  await page.keyboard.press('Escape');
  await expect(profile).toHaveCount(0);
  await expect(opensProfile.first()).toBeFocused();
  // Opened again while it slides away, held there mid-slide, its X takes focus again, and
  // let go, Escape closes it.
  await page.keyboard.press('Enter');
  await expect(profile.getByRole('button', { name: 'Close' })).toBeFocused();
  await page.keyboard.press('Escape');
  await page.evaluate(() => {
    for (const sliding of document.getAnimations()) sliding.pause();
  });
  await page.keyboard.press('Enter');
  await expect(profile.getByRole('button', { name: 'Close' })).toBeFocused();
  await page.evaluate(() => {
    for (const sliding of document.getAnimations()) sliding.play();
  });
  await page.keyboard.press('Escape');
  await expect(profile).toHaveCount(0);
  await expect(opensProfile.first()).toBeFocused();
  // Closed before its slide in has moved, held there at the edge, it's gone all the same.
  await page.keyboard.press('Enter');
  await page.evaluate(() => {
    for (const sliding of document.getAnimations()) {
      sliding.pause();
      sliding.currentTime = 0;
    }
  });
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

test('with two workspaces, each by its logo or initials, the lists show the one chosen, still chosen later, and Contacts opens a conversation', async ({
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
  // Operations' logo, which Dispatch keeps a copy of once it's connected.
  await expect
    .poll(async () => {
      const { workspaces } = await owner.read('/api/dsp/communications');
      return (workspaces as { teamId: string; logo: string | null }[]).some(
        (workspace) => workspace.teamId === 'T0FIXTURE1' && workspace.logo,
      );
    })
    .toBe(true);

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  const list = page.getByRole('region', { name: 'Conversations' });
  const jordan = list.getByRole('listitem').filter({ hasText: 'Jordan Ellis' });
  const cameron = list.getByRole('listitem').filter({ hasText: 'Cameron Hayes' });
  await expect(jordan).toBeVisible();
  await expect(cameron).toBeVisible();
  // Each workspace by its logo, or the first letters of its name without one.
  await list.getByLabel('Workspace: All workspaces').click();
  await expect(list.getByRole('button', { name: /^Operations/ }).locator('img')).toBeVisible();
  await expect(list.getByRole('button', { name: /^Drivers/ })).toContainText('DR');
  await expect(list.getByRole('button', { name: /^Drivers/ }).locator('img')).toHaveCount(0);
  // Drivers alone, which stays chosen when the page opens again.
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

test('an owner messages a group from New message, its answers come back together, and Reply opens one person’s own conversation', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  expect((await connect(owner, 'T0FIXTURE2')).status).toBe(200);

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  // Message a group comes first in New message, and Enter picks it.
  await page.getByRole('button', { name: 'New message' }).click();
  await expect(page.getByRole('option', { name: 'Message a group' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await page.keyboard.press('Enter');
  // To finds people by a misspelt name, each a chip; the message goes to each of them.
  const draft = page.getByRole('region', { name: 'New message' });
  const to = draft.getByRole('combobox', { name: 'To' });
  for (const typed of ['camron', 'casey']) {
    await to.fill(typed);
    await expect(draft.getByRole('option').first()).toBeVisible();
    await to.press('Enter');
  }
  await expect(draft.getByRole('button', { name: /^Remove/ })).toHaveCount(2);
  await draft.getByLabel('Message').fill('Vests are by the door');
  await draft.getByRole('button', { name: 'Send to 2' }).click();
  const group = page.getByRole('region', { name: 'Cameron and Casey' });
  await expect(group.getByText('Sent to 2')).toBeVisible();

  // An answer comes back under the group, which waits.
  const answer = fromSlack(
    dispatch,
    '/slack/events',
    event('T0FIXTURE2', {
      type: 'message',
      channel_type: 'im',
      channel: 'D0FIXCAM',
      user: 'U0FIXCAM',
      text: 'Copy that',
      ts: (Date.now() / 1000).toFixed(6),
    }),
  );
  expect((await answer).status).toBe(200);
  await expect(group.getByText('Copy that')).toBeVisible();
  const list = page.getByRole('region', { name: 'Conversations' });
  const row = list.getByRole('listitem').filter({ hasText: 'Cameron and Casey' });
  await expect(row).toContainText('Cameron: Copy that');
  await expect(list.getByRole('heading', { name: /^Waiting/ })).toBeVisible();

  // Reply opens Cameron's own conversation, where the group message carries its name, which
  // opens the group again.
  await group.getByText('Copy that').hover();
  await group.getByRole('button', { name: 'Reply to Cameron Hayes' }).click();
  const cameron = page.getByRole('region', { name: 'Cameron Hayes' });
  await cameron.getByRole('button', { name: 'Cameron and Casey' }).click();
  // Done marks everything waiting under it done.
  await group.getByRole('button', { name: 'Done' }).click();
  await expect(list.getByRole('heading', { name: /^Waiting/ })).toHaveCount(0);
});

test('Display opens the member’s own Message display, its preview made to fit, where Enter can start a new line, Ctrl+Enter still sends, and a kept profile opens with each conversation, beside the list in a 1024px window', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  expect((await connect(owner, 'T0FIXTURE2')).status).toBe(200);
  const written = await fromSlack(
    dispatch,
    '/slack/events',
    event('T0FIXTURE2', {
      type: 'message',
      channel_type: 'im',
      channel: 'D0FIXCASEY',
      user: 'U0FIXCASEY',
      text: 'Can I swap Saturday?',
      ts: (Date.now() / 1000).toFixed(6),
    }),
  );
  expect(written.status).toBe(200);

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  await page.getByRole('button', { name: 'Display' }).click();
  await expect(page.getByRole('heading', { name: 'Display', level: 1 })).toBeVisible();
  await expect(
    page.getByRole('radiogroup', { name: 'Size' }).getByRole('radio', { name: 'Fit' }),
  ).toBeChecked();
  // Saved as it's changed, the member's own.
  const saved = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/dsp/settings/communications.display') && response.ok(),
  );
  await page.getByRole('switch', { name: 'Enter sends' }).uncheck();
  await saved;

  await page.goBack();
  const casey = page.getByRole('region', { name: 'Casey Rivera' });
  const box = casey.getByLabel('Message');
  await box.fill('Yes');
  await box.press('Enter');
  await box.pressSequentially('Cam takes Sunday');
  await expect(box).toHaveValue('Yes\nCam takes Sunday');
  await box.press('Control+Enter');
  await expect(box).toHaveValue('');
  await expect(casey.getByText('Cam takes Sunday')).toBeVisible();

  // Kept open, a conversation's profile opens with it, though Settings is read after it opens.
  await page.getByRole('button', { name: 'Display' }).click();
  const kept = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/dsp/settings/communications.display') && response.ok(),
  );
  await page.getByRole('switch', { name: 'Keep profile open' }).check();
  await kept;
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route('**/api/dsp/settings', async (route) => {
    await held;
    await route.continue();
  });
  await page.goBack();
  await page.reload();
  await expect(box).toBeVisible();
  const profile = page.getByRole('dialog', { name: 'Casey Rivera' });
  await expect(profile).toHaveCount(0);
  release();
  await expect(profile).toBeVisible();

  // A 1024px window, the app's sidebar beside the page, still has the list beside the
  // conversation.
  await page.setViewportSize({ width: 1024, height: 768 });
  await expect(page.getByRole('region', { name: 'Conversations' })).toBeVisible();
  await expect(box).toBeVisible();
});

test('on a wide screen a bubble is as wide as chosen, plain lines stay readable, and large text widens the list and the profile, as Settings shows it, docking the profile only with room for the conversation', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  expect((await connect(owner, 'T0FIXTURE2')).status).toBe(200);
  const long =
    'The gate code at Riverside changed again this morning and the leasing office said they would text the new one, but nobody has it yet, so stops 40 through 58 may be late unless someone calls them before ten';
  const written = await fromSlack(
    dispatch,
    '/slack/events',
    event('T0FIXTURE2', {
      type: 'message',
      channel_type: 'im',
      channel: 'D0FIXCASEY',
      user: 'U0FIXCASEY',
      text: long,
      ts: (Date.now() / 1000).toFixed(6),
    }),
  );
  expect(written.status).toBe(200);
  // Changes to the owner's own Message display, as Settings saves them, which the page draws once
  // it's read them again (`drawn`).
  const look = page.locator('.communications');
  const display = async (values: Record<string, unknown>, drawn: RegExp) => {
    const { revision } = (await owner.read('/api/dsp/settings')).sections.find(
      (each: { id: string }) => each.id === 'communications.display',
    );
    const saved = await owner.post('/api/dsp/settings/communications.display', {
      values,
      revision,
    });
    expect(saved.status, saved.body).toBe(200);
    await page.reload();
    await expect(look).toHaveAttribute('style', drawn);
  };

  await page.setViewportSize({ width: 1920, height: 1080 });
  await login(page);
  await page.goto(`/#dsp/${north.id}/communications`);
  const list = page.getByRole('region', { name: 'Conversations' });
  await list.getByRole('button', { name: /^Casey Rivera/ }).click();
  const casey = page.getByRole('region', { name: 'Casey Rivera' });
  const bubble = casey.getByText(long);
  const width = async () => (await bubble.boundingBox())!.width;
  const usual = await width();
  await display({ bubble_width: 90 }, /--bubble-width: 90%/);
  expect(await width()).toBeGreaterThan(usual);
  await display({ bubble_width: 50 }, /--bubble-width: 50%/);
  expect(await width()).toBeLessThan(usual);
  await display({ bubble_width: 70, style: 'plain' }, /--bubble-width: 70%/);
  await expect(look).toHaveAttribute('data-style', 'plain');
  expect(await width()).toBeLessThanOrEqual(usual + 1);

  // At 14px the profile is a column of its own beside the conversation; at 20px the list and the
  // profile grow with the text, and the profile goes over the conversation, which keeps its room.
  const profile = page.getByRole('dialog', { name: 'Casey Rivera' });
  const opened = async () => {
    await casey
      .locator('.thread-head')
      .getByRole('button', { name: /^Casey Rivera/ })
      .click();
    await expect(profile).toBeVisible();
    // Once it, and the conversation making room for it, have slid.
    await profile.evaluate((panel) =>
      Promise.all(
        [panel, panel.closest('.pane-thread')!]
          .flatMap((each) => each.getAnimations())
          .map((each) => each.finished),
      ),
    );
    const thread = (await casey.locator('.thread').boundingBox())!;
    const panel = (await profile.boundingBox())!;
    return { beside: thread.x + thread.width <= panel.x + 1, width: panel.width };
  };
  await display({ style: 'bubbles' }, /--text-size: 14px/);
  await expect(look).toHaveAttribute('data-style', 'bubbles');
  const listAt14 = (await list.boundingBox())!.width;
  const at14 = await opened();
  expect(at14.beside).toBe(true);
  await display({ text_size: 20 }, /--text-size: 20px/);
  expect((await list.boundingBox())!.width).toBeGreaterThan(listAt14);
  const at20 = await opened();
  expect(at20.beside).toBe(false);
  expect(at20.width).toBeGreaterThan(at14.width);

  // Settings shows the profile's width as drawn, and saves the width it's chosen as at 14px.
  await page.getByRole('button', { name: 'Display' }).click();
  const drawn = `${Math.round(at20.width)} px`;
  const profileWidth = page.getByRole('slider', { name: 'Width', exact: true });
  await expect(profileWidth).toHaveAttribute('aria-valuetext', drawn);
  await expect(page.getByText(drawn, { exact: true })).toBeVisible();
  const saved = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/dsp/settings/communications.display') && response.ok(),
  );
  await profileWidth.fill('400');
  await saved;
  const { values } = (await owner.read('/api/dsp/settings')).sections.find(
    (each: { id: string }) => each.id === 'communications.display',
  );
  expect(values.profile_width).toBe(400);
});

test.describe('on a phone', () => {
  test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  test('Communications draws by the phone’s own Message display, where Return starts a new line, Send sends, its text size is the phone’s alone, and its header keeps a line each', async ({
    page,
    dispatch,
  }) => {
    const owner = await dispatch.client();
    const north = (owner.session.dsps as { id: string; name: string }[]).find(
      (dsp) => dsp.name === 'Northline Logistics',
    )!;
    await owner.select(north.id);
    expect((await connect(owner, 'T0FIXTURE2')).status).toBe(200);
    const written = await fromSlack(
      dispatch,
      '/slack/events',
      event('T0FIXTURE2', {
        type: 'message',
        channel_type: 'im',
        channel: 'D0FIXCASEY',
        user: 'U0FIXCASEY',
        text: 'Can I swap Saturday?',
        ts: (Date.now() / 1000).toFixed(6),
      }),
    );
    expect(written.status).toBe(200);

    await login(page);
    await page.goto(`/#dsp/${north.id}/communications`);
    await page.getByRole('button', { name: 'Display' }).click();
    // A phone's own, to start.
    await expect(page.getByRole('radio', { name: 'Phone' })).toBeChecked();
    const saved = page.waitForResponse(
      (response) =>
        response.url().endsWith('/api/dsp/settings/communications.display') && response.ok(),
    );
    await page.getByRole('slider', { name: 'Text size' }).fill('20');
    await saved;

    await page.goBack();
    await page
      .getByRole('region', { name: 'Conversations' })
      .getByRole('button', { name: /^Casey Rivera/ })
      .click();
    const casey = page.getByRole('region', { name: 'Casey Rivera' });
    const asked = casey.getByText('Can I swap Saturday?');
    await expect(asked).toHaveCSS('font-size', '20px');
    // Its header keeps a line each at that size: what they do and where, and Done, its mark alone.
    const about = await casey.locator('.thread-head .who small').evaluate((element) => ({
      height: element.getBoundingClientRect().height,
      size: parseFloat(getComputedStyle(element).fontSize),
    }));
    expect(about.height).toBeLessThan(about.size * 2);
    expect(await casey.getByRole('button', { name: 'Done' }).innerText()).toBe('');
    const box = casey.getByLabel('Message');
    await box.fill('Yes');
    await box.press('Enter');
    await box.pressSequentially('Cam takes Sunday');
    await expect(box).toHaveValue('Yes\nCam takes Sunday');
    await casey.getByRole('button', { name: 'Send' }).click();
    await expect(box).toHaveValue('');
    await expect(casey.getByText('Cam takes Sunday')).toBeVisible();

    // The same member on a desktop's screen keeps the desktop's text size.
    await page.setViewportSize({ width: 1280, height: 800 });
    await expect(asked).toHaveCSS('font-size', '14px');
  });
});
