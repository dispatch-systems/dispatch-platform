import {
  test,
  expect,
  demo,
  login,
  openDsp,
} from '../../../../core/shell/tests/support/fixtures.js';
import { connect, event, fromSlack, install } from '../../../slack/tests/support/slack.js';

// The seeded DSP texts through its demo's stand-in phone, (206) 555-0100, which received three
// texts and delivers what it's given at once; it has no Slack until a test connects it. Routes is
// on, and Live Route Data, which reads a provider, off: Today's Routes lists nobody.

/** A picture of a single pixel, as a PNG. */
const PIXEL = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==',
  'base64',
);

test('with its phone and no Slack, Communications opens on Recents, where its texts wait, beside Today’s Routes, with no routes before live route data reads today: a number saved as a contact, asked before a name is left unsaved, added to a driver and taken off them, answered by text with a picture, blocked into Contacts and unblocked', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  // It opens on Recents, the first section, which lists the texts.
  const sections = page.getByRole('tablist', { name: 'Sections' });
  const recents = sections.getByRole('tab', { name: /Recents/ });
  await expect(recents).toHaveAttribute('aria-selected', 'true');
  await sections.getByRole('tab', { name: "Today's Routes" }).click();
  await expect(page.getByText('No routes', { exact: true })).toBeVisible();
  // Nothing of Slack.
  await expect(sections.getByRole('tab', { name: /Slack/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Connect Slack' })).toHaveCount(0);
  await recents.click();
  const list = page.getByRole('region', { name: 'Conversations' });
  const stranger = list.getByRole('listitem').filter({ hasText: '(425) 555-0199' });
  await expect(stranger).toContainText('Hi is this dispatch? I start Monday');
  await stranger.getByRole('button').first().click();
  const saveContact = page
    .getByRole('region', { name: '(425) 555-0199' })
    .getByRole('button', { name: 'Save contact' });
  const name = page.getByLabel('Name', { exact: true });
  const asking = page.getByRole('dialog', { name: 'Save changes?' });
  // Left by the keyboard with nothing written, focus goes back to what opened it.
  await saveContact.click();
  await page.keyboard.press('Escape');
  await expect(name).toHaveCount(0);
  await expect(saveContact).toBeFocused();
  // A name written and left asks first, from the other tab too: discarded, it's gone; saved,
  // it's the contact's.
  const newContact = page.getByRole('tab', { name: 'New contact' });
  await saveContact.click();
  await newContact.click();
  await name.fill('Jamie Ortiz');
  await page.keyboard.press('Escape');
  await asking.getByRole('button', { name: 'Discard changes' }).click();
  await expect(name).toHaveCount(0);
  await expect(stranger).toBeVisible();
  await saveContact.click();
  await newContact.click();
  await name.fill('Jamie Ortiz');
  await page.getByRole('tab', { name: 'Add to driver' }).click();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await asking.getByRole('button', { name: 'Save changes' }).click();
  await expect(list.getByRole('listitem').filter({ hasText: 'Jamie Ortiz' })).toBeVisible();

  // Added to a driver, a number's texts are theirs, until it's taken off them again.
  const gate = list.getByRole('listitem').filter({ hasText: '(425) 555-0131' });
  await gate.getByRole('button').first().click();
  const unknown = page.getByRole('region', { name: '(425) 555-0131' });
  await unknown.getByRole('button', { name: 'Save contact' }).click();
  await page.getByRole('button', { name: 'Alex Parker' }).click();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  const alex = page.getByRole('region', { name: 'Alex Parker' });
  await expect(alex.getByRole('button', { name: 'Save contact' })).toHaveCount(0);
  await alex.getByLabel('More').click();
  await alex.getByRole('button', { name: 'Remove from Alex Parker' }).click();
  await expect(unknown.getByRole('button', { name: 'Save contact' })).toBeVisible();
  await list
    .getByRole('listitem')
    .filter({ hasText: 'Jamie Ortiz' })
    .getByRole('button')
    .first()
    .click();

  // Answered by text, it goes from the DSP's phone, which delivers it.
  const jamie = page.getByRole('region', { name: 'Jamie Ortiz' });
  await jamie.getByLabel('Message').fill('See you Monday at 7');
  await jamie.getByLabel('Message').press('Enter');
  await expect(jamie.getByText('Delivered')).toBeVisible();
  // Recents lists the texts still waiting under Waiting, above it, now answered, under Today.
  const headings = list.getByRole('heading');
  await expect(headings.first()).toHaveText(/^Waiting/);
  await expect(headings.nth(1)).toHaveText('Today');
  // A picture goes by text too, kept by the phone, and shows as sent.
  const choosing = page.waitForEvent('filechooser');
  await jamie.getByRole('button', { name: 'Attach', exact: true }).click();
  await jamie.getByRole('button', { name: 'Attach Image' }).click();
  await (await choosing).setFiles({ name: 'door.png', mimeType: 'image/png', buffer: PIXEL });
  await jamie.getByRole('button', { name: 'Send' }).click();
  // Pressed, it opens over the page, focus inside it, and Escape closes it, focus back on it.
  // Named for its picture alone once that loads: shown as a file before then, its size or state
  // is in its name too.
  const door = jamie.getByRole('button', { name: 'door.png', exact: true });
  await door.click();
  const viewer = page.getByRole('dialog', { name: 'door.png' });
  await expect(viewer.getByRole('img', { name: 'door.png' })).toBeVisible();
  await expect(viewer.getByRole('button', { name: 'Download' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(viewer).toHaveCount(0);
  await expect(door).toBeFocused();

  // Blocked once asked, it leaves Messages for Contacts' Blocked, until it's unblocked.
  await jamie.getByLabel('More').click();
  await jamie.getByRole('button', { name: 'Block', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Block (425) 555-0199?' })
    .getByRole('button', { name: 'Block' })
    .click();
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

test('Today’s Routes reads before it says No routes, a read that fails says so in Today’s Routes alone, and offline it waits to read', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  // Its first read waits until the test lets it go.
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route('**/api/dsp/communications/routes', async (route) => {
    await held;
    await route.continue();
  });
  await page.getByRole('link', { name: 'Communications' }).click();
  // It opens on Recents, where the DSP's texts wait, which reads Today's Routes as it shows.
  const sections = page.getByRole('tablist', { name: 'Sections' });
  await sections.getByRole('tab', { name: "Today's Routes" }).click();
  const list = page.getByRole('region', { name: 'Conversations' });
  await expect(list.getByRole('status')).toContainText('Loading');
  await expect(page.getByText('No routes', { exact: true })).toHaveCount(0);
  release();
  await expect(page.getByText('No routes', { exact: true })).toBeVisible();

  // Read again, as the page comes back into view, it fails: Today's Routes says so, Recents
  // doesn't.
  await page.unroute('**/api/dsp/communications/routes');
  await page.route('**/api/dsp/communications/routes', (route) =>
    route.fulfill({ status: 500, json: { error: 'internal', message: 'Something went wrong.' } }),
  );
  await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await expect(list.getByRole('alert')).toHaveText('Something went wrong.');
  await sections.getByRole('tab', { name: /Recents/ }).click();
  await expect(list).toContainText('(425) 555-0199');
  await expect(page.getByRole('alert')).toHaveCount(0);

  // Opened again on Recents, as chosen, then offline while Today's Routes' first read is on its
  // way: it says it'll read on reconnecting, never No routes, and shows the day once that read
  // answers back online.
  await page.unroute('**/api/dsp/communications/routes');
  let answer!: () => void;
  const answered = new Promise<void>((resolve) => (answer = resolve));
  await page.route('**/api/dsp/communications/routes', async (route) => {
    await answered;
    await route.continue();
  });
  await page.reload();
  await expect(list).toContainText('(425) 555-0199');
  await page.context().setOffline(true);
  await sections.getByRole('tab', { name: "Today's Routes" }).click();
  await expect(list.getByRole('status')).toContainText('offline');
  await expect(page.getByText('No routes', { exact: true })).toHaveCount(0);
  await page.context().setOffline(false);
  answer();
  await expect(page.getByText('No routes', { exact: true })).toBeVisible();
});

test('with Slack too, the sections switch, each with how many wait, stay as chosen, and a link opens its section unless Recents shows and lists it, the board chosen or not', async ({
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
  const recents = sections.getByRole('tab', { name: /Recents/ });
  const slack = sections.getByRole('tab', { name: /Slack/ });
  // Recents counts everyone waiting: the texts and Slack's.
  await expect(recents).toHaveText('Recents4');
  await expect(slack).toHaveText('Slack1');
  const list = page.getByRole('region', { name: 'Conversations' });
  await recents.click();
  await expect(list).toContainText('(425) 555-0199');
  await expect(list.getByRole('listitem').filter({ hasText: 'Jordan Ellis' })).toBeVisible();
  await slack.click();
  await expect(list.getByRole('listitem').filter({ hasText: 'Jordan Ellis' })).toBeVisible();
  await expect(list).not.toContainText('(425) 555-0199');
  await page.reload();
  await expect(slack).toHaveAttribute('aria-selected', 'true');

  // A link to a text, as a new one's notification is, opens it in Recents.
  const { conversations } = await owner.read('/api/dsp/communications');
  const text = (conversations as { id: string; number: string | null }[]).find(
    (each) => each.number === '+14255550199',
  )!;
  const toText = `/#dsp/${north.id}/communications?open=${text.id}`;
  await page.goto(toText);
  await expect(page.getByRole('region', { name: '(425) 555-0199' })).toBeVisible();
  await expect(recents).toHaveAttribute('aria-selected', 'true');

  // With the board chosen in Today's Routes (Routes, beside two others), a link to one the list
  // doesn't show, as a conversation just started, still opens it, in its section.
  await sections.getByRole('tab', { name: /Routes/ }).click();
  await page.getByRole('button', { name: 'Board' }).click();
  await expect(page.getByRole('region', { name: "Today's Routes" })).toBeVisible();
  const started = await owner.post('/api/dsp/communications/texts/conversations', {
    number: '2065550123',
  });
  expect(started.status, started.body).toBe(200);
  await page.goto(`/#dsp/${north.id}/communications?open=${started.value.id}`);
  await expect(page.getByRole('region', { name: '(206) 555-0123' })).toBeVisible();
  await expect(recents).toHaveAttribute('aria-selected', 'true');

  // From Today's Routes, which lists no group, a link to a group opens it in Slack, and one to the
  // text again, Recents; from Recents, which lists groups too, a link to the group stays there.
  const group = await owner.post('/api/dsp/communications/groups', {
    name: 'Morning wave',
    people: [{ teamId: 'T0FIXTURE1', userId: 'U0FIXJORDAN' }],
    text: 'Running 20 minutes late',
    photos: [],
  });
  expect(group.status, group.body).toBe(201);
  const toGroup = `/#dsp/${north.id}/communications?group=${group.value.id}`;
  await sections.getByRole('tab', { name: /Routes/ }).click();
  await page.goto(toGroup);
  await expect(page.getByRole('region', { name: 'Morning wave' })).toBeVisible();
  await expect(slack).toHaveAttribute('aria-selected', 'true');
  await page.goto(toText);
  await expect(page.getByRole('region', { name: '(425) 555-0199' })).toBeVisible();
  await expect(recents).toHaveAttribute('aria-selected', 'true');
  await page.goto(toGroup);
  await expect(page.getByRole('region', { name: 'Morning wave' })).toBeVisible();
  await expect(recents).toHaveAttribute('aria-selected', 'true');
});

test('the section switch keeps its three tabs on one line, each whole, whichever of Settings and Communications opens first', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  expect((await connect(owner, 'T0FIXTURE1')).status).toBe(200);
  const communications = `/#dsp/${north.id}/communications`;
  // Message display draws the page's own parts, so its page loads their stylesheets too.
  const display = `/#dsp/${north.id}/settings?tab=display`;
  /** Each of the three section tabs on one line with the others, its label whole. */
  const inLine = async () => {
    const tabs = page.getByRole('tablist', { name: 'Sections' }).getByRole('tab');
    await expect(tabs).toHaveCount(3);
    const drawn = await tabs.evaluateAll((all) =>
      all.map((tab) => ({
        top: Math.round(tab.getBoundingClientRect().top),
        whole: tab.scrollWidth <= tab.clientWidth,
      })),
    );
    expect(drawn).toEqual(drawn.map(() => ({ top: drawn[0]!.top, whole: true })));
  };

  await login(page);
  await page.goto(display);
  await expect(page.getByRole('heading', { name: 'Display', level: 1 })).toBeVisible();
  await page.goto(communications);
  await inLine();
  // Opened afresh, Communications first, then Message display, then back.
  await page.reload();
  await inLine();
  await page.goto(display);
  await expect(page.getByRole('heading', { name: 'Display', level: 1 })).toBeVisible();
  await page.goto(communications);
  await inLine();
});

test('with Slack needing an update, Recents still works, and Update Slack covers Slack alone until closed', async ({
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
  const sections = page.getByRole('tablist', { name: 'Sections' });
  await sections.getByRole('tab', { name: /Recents/ }).click();
  const stranger = page
    .getByRole('region', { name: 'Conversations' })
    .getByRole('listitem')
    .filter({ hasText: '(425) 555-0199' });
  await expect(stranger).toBeVisible();
  await expect(update).toHaveCount(0);
  await stranger.getByRole('button').first().click();
  await expect(page.getByRole('region', { name: '(425) 555-0199' })).toBeVisible();

  // Slack waits on the update; closed, Recents shows again.
  await sections.getByRole('tab', { name: /Slack/ }).click();
  await expect(update.getByRole('listitem')).toHaveText(/Operations/);
  await update.getByRole('button', { name: 'Close dialog' }).click();
  await expect(update).toHaveCount(0);
  await expect(sections.getByRole('tab', { name: /Recents/ })).toHaveAttribute(
    'aria-selected',
    'true',
  );
});

test('a teammate typing shows above the box, and their reply since the member started writing, with text or a photo, is asked about before sending, also once an earlier send ends, until the draft starts again, and sent anyway only once it can go', async ({
  page,
  dispatch,
}) => {
  // The demo's member, a teammate here once their role has Communications.
  const owner = await dispatch.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const role = (await owner.get('/api/dsp/roles')).value.find(
    (r: { name: string }) => r.name === 'Member',
  );
  const granted = await owner.post(`/api/dsp/roles/${role.id}`, {
    name: 'Member',
    permissions: [...role.permissions, 'communications.use'],
  });
  expect(granted.status).toBe(200);
  const teammate = await dispatch.client(demo.member);
  await teammate.select(north.id);
  const { conversations } = await teammate.read('/api/dsp/communications');
  const id = conversations.find((c: { number: string }) => c.number === '+14255550199').id;

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  await page
    .getByRole('region', { name: 'Conversations' })
    .getByRole('listitem')
    .filter({ hasText: '(425) 555-0199' })
    .getByRole('button')
    .first()
    .click();
  const conversation = page.getByRole('region', { name: '(425) 555-0199' });
  const box = conversation.getByLabel('Message');
  const send = conversation.getByRole('button', { name: 'Send', exact: true });
  await expect(box).toBeEnabled();
  expect((await teammate.post(`/api/dsp/communications/conversations/${id}/typing`)).status).toBe(
    200,
  );
  await expect(conversation.getByRole('status').filter({ hasText: 'is typing…' })).toBeVisible();

  // The member's send held, they write again, and the teammate replies before it ends: what
  // they wrote since is still asked about.
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route('**/api/dsp/communications/conversations/*/messages', async (route) => {
    if (route.request().method() === 'POST') await held;
    await route.continue();
  });
  await box.fill('On my way');
  await send.click();
  await expect(box).toHaveValue('');
  await box.fill('The gate code is 4412');
  const reply = await teammate.post(`/api/dsp/communications/conversations/${id}/messages`, {
    text: 'Calling them now',
  });
  expect(reply.status).toBe(201);
  await expect(conversation.getByText('Calling them now')).toBeVisible();
  release();
  await expect(conversation.getByText('On my way')).toBeVisible();
  await send.click();
  const asking = conversation.getByRole('alert');
  await expect(asking).toContainText('just replied');

  // Send anyway goes only as Send would: not with nothing to send. Written anew, the draft
  // starts again, no longer asked about, and Send sends it.
  const anyway = asking.getByRole('button', { name: 'Send anyway' });
  await box.fill('');
  await expect(anyway).toBeDisabled();
  await box.fill('Gate code is 4412');
  await expect(asking).toHaveCount(0);
  await send.click();
  await expect(conversation.getByText('Gate code is 4412')).toBeVisible();
  await expect(asking).toHaveCount(0);

  // A photo starts a draft too: a reply before its caption is still asked about, and it goes
  // anyway.
  const choosing = page.waitForEvent('filechooser');
  await conversation.getByRole('button', { name: 'Attach', exact: true }).click();
  await conversation.getByRole('button', { name: 'Attach Image' }).click();
  await (await choosing).setFiles({ name: 'gate.png', mimeType: 'image/png', buffer: PIXEL });
  const again = await teammate.post(`/api/dsp/communications/conversations/${id}/messages`, {
    text: 'Sending them a photo too',
  });
  expect(again.status).toBe(201);
  await expect(conversation.getByText('Sending them a photo too')).toBeVisible();
  await box.fill('This gate');
  await send.click();
  await expect(asking).toContainText('just replied');
  await anyway.click();
  await expect(asking).toHaveCount(0);
  await expect(conversation.getByText('This gate')).toBeVisible();
});
