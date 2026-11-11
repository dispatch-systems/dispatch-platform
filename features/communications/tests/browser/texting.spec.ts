import type { Page } from '@playwright/test';
import type { Communications, ConversationWindow, Message } from '../../api/index.js';
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

test('without Slack, a number not saved as a contact can be texted first and opened again in another format', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Dispatch Phone' }).click();
  await page.getByRole('button', { name: 'New message' }).click();
  const dialog = page.getByRole('dialog', { name: 'New message' });
  const number = dialog.getByRole('combobox', { name: 'Phone number', exact: true });
  await expect(dialog.getByRole('option', { name: 'Message a group' })).toHaveCount(0);
  await number.fill('2025550177');
  await expect(dialog.getByRole('option', { name: 'Text (202) 555-0177' })).toBeVisible();
  await number.press('Enter');
  const conversation = page.getByRole('region', { name: '(202) 555-0177', exact: true });
  await expect(conversation.getByRole('button', { name: 'Save contact' })).toBeVisible();
  await conversation.getByLabel('Message', { exact: true }).fill('Hello from dispatch');
  await conversation.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(conversation.getByText('Delivered', { exact: true })).toBeVisible();
  const address = page.url();
  await page.getByRole('button', { name: 'New message' }).click();
  await number.fill('+1 (202) 555-0177');
  await dialog.getByRole('option', { name: 'Text (202) 555-0177' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page).toHaveURL(address);
  await expect(conversation.getByText('Hello from dispatch', { exact: true })).toBeVisible();
  await expect(conversation.getByRole('button', { name: 'Save contact' })).toBeVisible();
});

test('with its phone and no Slack, Communications opens on Recents, where its texts wait, beside Today’s Routes, with no routes before live route data reads today: a number saved as a contact, asked before a name is left unsaved, added to a driver and taken off them, answered by text with a picture, blocked into Contacts and unblocked', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Dispatch Phone' }).click();
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
  // Its menu unblocks it at once, before the server has; blocked again, it waits in Contacts.
  const unblockingHere = '**/conversations/*/unblock';
  let answer = () => {};
  const answering = new Promise<void>((resolve) => (answer = resolve));
  await page.route(unblockingHere, async (route) => {
    await answering;
    await route.continue();
  });
  await jamie.getByLabel('More').click();
  await jamie.getByRole('button', { name: 'Unblock', exact: true }).click();
  await jamie.getByLabel('More').click();
  await expect(jamie.getByRole('button', { name: 'Block', exact: true })).toBeVisible();
  const unblocked = page.waitForResponse((response) => response.url().endsWith('/unblock'));
  answer();
  await unblocked;
  await page.unroute(unblockingHere);
  await jamie.getByRole('button', { name: 'Block', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Block (425) 555-0199?' })
    .getByRole('button', { name: 'Block' })
    .click();
  await expect(list.getByRole('listitem').filter({ hasText: 'Jamie Ortiz' })).toHaveCount(0);
  await page.getByRole('tab', { name: 'Contacts' }).click();
  const contacts = page.getByRole('region', { name: 'Contacts' });
  // Unblocked, it leaves Blocked at once, before the server has.
  const unblocking = '**/conversations/*/unblock';
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(unblocking, async (route) => {
    await held;
    await route.continue();
  });
  await contacts
    .getByRole('listitem')
    .filter({ hasText: 'Jamie Ortiz' })
    .getByRole('button', { name: 'Unblock' })
    .click();
  await expect(contacts.getByRole('button', { name: 'Unblock' })).toHaveCount(0);
  release();
  await page.unroute(unblocking);
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
  await page.getByRole('link', { name: 'Dispatch Phone' }).click();
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
  await page.getByRole('link', { name: 'Dispatch Phone' }).click();
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
  const toText = `/#dsp/${north.id}/dispatch-phone?open=${text.id}`;
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
  const toGroup = `/#dsp/${north.id}/dispatch-phone?group=${group.value.id}`;
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
  await page.getByRole('link', { name: 'Dispatch Phone' }).click();
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
  await page.getByRole('link', { name: 'Dispatch Phone' }).click();
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

test('bounded text windows keep the visible part anchored and defer eviction until the photo viewer and its focused opener release it', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.route('**/files/bounded-photo**', (route) =>
    route.fulfill({ contentType: 'image/png', body: PIXEL }),
  );
  const all = textHistory();
  let release = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let requested = () => {};
  const request = new Promise<void>((resolve) => {
    requested = resolve;
  });
  let shifts = 0;
  await page.route('**/conversations/*/window?*', async (route) => {
    const response = await route.fetch();
    const source = (await response.json()) as ConversationWindow;
    if (source.conversation.number !== '+14255550199') return route.fulfill({ response });
    const url = new URL(route.request().url());
    const anchor = url.searchParams.get('anchor');
    if (anchor) {
      shifts++;
      requested();
      await held;
    }
    const answer = textWindow(source, all, url, true);
    await route.fulfill({ json: answer });
  });
  await page.getByRole('link', { name: 'Dispatch Phone' }).click();
  const list = page.getByRole('region', { name: 'Conversations' });
  await list
    .getByRole('listitem')
    .filter({ hasText: '(425) 555-0199' })
    .getByRole('button')
    .first()
    .click();
  const pane = page.getByRole('region', { name: '(425) 555-0199' });
  const thread = pane.locator('.thread.windowed');
  const bubbles = thread.locator('.bubble[data-message-id]');
  await expect(bubbles).toHaveCount(150);
  const photo = pane.getByRole('button', { name: 'door.png', exact: true });
  await thread.locator('[data-message-part="file:bounded-photo"]').scrollIntoViewIfNeeded();
  await expect(photo).toBeEnabled();
  await photo.click();
  const opener = await photo.elementHandle();
  const viewer = page.getByRole('dialog', { name: 'door.png' });
  await expect(viewer.getByRole('img', { name: 'door.png' })).toBeVisible();
  await thread.evaluate((element) => {
    element.scrollTop = 0;
  });
  await page.keyboard.press('Escape');
  await expect(photo).toBeFocused();
  expect(
    await opener!.evaluate((element) => element.isConnected && document.activeElement === element),
  ).toBe(true);
  expect(shifts).toBe(0);
  await pane.getByLabel('Message', { exact: true }).click();
  await thread.evaluate((element) => {
    element.scrollTop = 0;
    element.dispatchEvent(new Event('scroll'));
  });
  await request;
  // Move after dispatch: restoration must use the current viewport, not the request's old offset.
  await thread.evaluate((element) => {
    element.scrollTop = 35;
  });
  const anchor = await thread.evaluate((element) => {
    const top = element.getBoundingClientRect().top;
    const part = [
      ...element.querySelectorAll<HTMLElement>('[data-message-id][data-message-part]'),
    ].find((part) => part.getBoundingClientRect().bottom > top)!;
    return {
      id: part.dataset.messageId!,
      part: part.dataset.messagePart!,
      top: part.getBoundingClientRect().top - top,
    };
  });
  release();
  await expect(
    thread.locator('[data-message-id="bounded-200"][data-message-part="text"]'),
  ).toHaveCount(1);
  await expect(bubbles).toHaveCount(150);
  const retained = thread.locator(
    `[data-message-id="${anchor.id}"][data-message-part="${anchor.part}"]`,
  );
  await expect
    .poll(async () =>
      retained.evaluate(
        (part) =>
          part.getBoundingClientRect().top - part.closest('.thread')!.getBoundingClientRect().top,
      ),
    )
    .toBeCloseTo(anchor.top, 0);
  expect(
    new Set(
      await bubbles.evaluateAll((elements) =>
        elements.map((element) => element.getAttribute('data-message-id')),
      ),
    ).size,
  ).toBe(150);
  await pane.getByLabel('Message', { exact: true }).fill('Draft survives paging');
  await thread.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(
    thread.locator('[data-message-id="bounded-399"][data-message-part="text"]'),
  ).toHaveCount(1);
  await expect(bubbles).toHaveCount(150);
  await expect(pane.getByLabel('Message', { exact: true })).toHaveValue('Draft survives paging');
});

function eventGate() {
  let release!: () => void;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release };
}

function textHistory(media = 380): Message[] {
  return Array.from({ length: 400 }, (_, index) => ({
    id: `bounded-${index}`,
    ts: `${Date.UTC(2026, 9, 26, 10, index)}:bounded-${index}`,
    at: Date.UTC(2026, 9, 26, 10, index),
    direction: 'in',
    text: `Bounded ${index}\n${'line\n'.repeat(index % 7)}`,
    editedAt: null,
    inThread: false,
    sender: null,
    gif: null,
    group: null,
    status: null,
    error: null,
    number: '+14255550199',
    moved: false,
    files:
      index === media
        ? [
            {
              id: 'bounded-photo',
              name: 'door.png',
              mime: 'image/png',
              size: PIXEL.length,
              status: 'kept',
              version: 'bounded',
              preview: true,
            },
          ]
        : [],
  }));
}

/** These histories have one date and no sender-bearing rows. Run starts still respect an
 * outgoing retry, rather than assigning the incoming origin to every selected first row. */
function textWindow(
  source: ConversationWindow,
  all: Message[],
  url: URL,
  ready: boolean,
): ConversationWindow {
  const anchor = url.searchParams.get('anchor');
  const index = anchor ? all.findIndex((row) => row.ts === anchor) : all.length;
  expect(index).toBeGreaterThanOrEqual(0);
  const start = Math.max(
    0,
    Math.min(250, anchor ? index - Number(url.searchParams.get('leading')) : 250),
  );
  const rows = all.slice(start, start + 150);
  const neighbor = (row?: Message) =>
    row
      ? {
          id: row.id,
          ts: row.ts,
          at: row.at,
          direction: row.direction,
          sender: row.sender,
          groupId: row.group?.id ?? null,
          number: row.number,
        }
      : null;
  const origin = (at: number) => {
    while (
      at > 0 &&
      all[at - 1]!.direction === all[at]!.direction &&
      all[at - 1]!.number === all[at]!.number &&
      all[at]!.at - all[at - 1]!.at < 600_000
    )
      at--;
    const first = all[at]!;
    return {
      origin: {
        id: first.id,
        ts: first.ts,
        at: first.at,
        direction: first.direction,
        sender: first.sender,
        group: first.group,
        number: first.number,
      },
      beforeOrigin: neighbor(all[at - 1]),
      sentBeforeOrigin: null,
      firstDay: { id: all[0]!.id, ts: all[0]!.ts, at: all[0]!.at, number: all[0]!.number },
    };
  };
  return {
    ...source,
    messages: rows,
    latest: all.slice(-50),
    context: {
      previous: neighbor(all[start - 1]),
      next: neighbor(all[start + 150]),
      sentBefore: null,
    },
    latestContext: { previous: neighbor(all[349]), next: null, sentBefore: null },
    latestWorkdays: [],
    workdays: [],
    origins: {
      ready,
      version: 1,
      algorithm: 'conversation-origins-1:2026a',
      timeZone: source.origins?.timeZone ?? 'America/Los_Angeles',
      selected: ready ? origin(start) : null,
      latest: ready ? origin(350) : null,
    },
    range: {
      ...source.range,
      first: rows[0]!.ts,
      last: rows.at(-1)!.ts,
      hasBefore: start > 0,
      hasAfter: start + 150 < all.length,
    },
  };
}

async function textFixture(page: Page, all: Message[], ready = false) {
  let source: ConversationWindow | undefined;
  let revision = 0;
  const stopped = eventGate();
  const fixture = {
    ready,
    windows: [] as URL[],
    beforeWindow: undefined as (() => Promise<void>) | undefined,
    beforeLegacy: undefined as (() => Promise<void>) | undefined,
  };
  // Freeze background updates so the single explicit invalidation is the only refresh trigger.
  await page.route('**/api/dsp/communications/updates?*', async (route) => {
    await stopped.promise;
    await route.abort();
  });
  await page.route('**/api/dsp/communications', async (route) => {
    const response = await route.fetch();
    const value = (await response.json()) as Communications;
    await route.fulfill({ json: { ...value, revision: value.revision + revision } });
  });
  await page.route('**/conversations/*/window?*', async (route) => {
    const response = await route.fetch();
    const value = (await response.json()) as ConversationWindow;
    if (value.conversation.number !== '+14255550199') return route.fulfill({ response });
    source = value;
    const url = new URL(route.request().url());
    fixture.windows.push(url);
    const answer = textWindow(value, all, url, fixture.ready);
    await fixture.beforeWindow?.();
    await route.fulfill({ json: answer });
  });
  await page.route(
    /\/api\/dsp\/communications\/conversations\/[^/?]+(?:\?[^#]*)?$/,
    async (route) => {
      if (!source || !route.request().url().includes(`/conversations/${source.conversation.id}`))
        return route.continue();
      const url = new URL(route.request().url());
      const before = url.searchParams.get('before');
      const end = before ? all.findIndex((row) => row.ts === before) : all.length;
      const start = Math.max(0, end - 50);
      await fixture.beforeLegacy?.();
      await route.fulfill({
        json: { ...source, messages: all.slice(start, end), more: start > 0 },
      });
    },
  );
  await page.route('**/files/bounded-photo**', (route) =>
    route.fulfill({ contentType: 'image/png', body: PIXEL }),
  );
  return {
    fixture,
    // The page asks for a window again as its updates end. A request still on its way as the
    // test ends is let go here, where its handler would otherwise read an answer the closing
    // browser has thrown away, and fail a test that had passed.
    stop: async () => {
      stopped.release();
      await page.unrouteAll({ behavior: 'ignoreErrors' });
    },
    invalidate: async () => {
      revision++;
      const response = page.waitForResponse(
        (response) => new URL(response.url()).pathname === '/api/dsp/communications',
      );
      await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
      await (await response).finished();
      // A rendering event, never an elapsed wait: the inbox publication and its effects may run.
      await page.evaluate(
        () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())),
      );
    },
  };
}

async function openText(page: Page) {
  await page.getByRole('link', { name: 'Dispatch Phone' }).click();
  const list = page.getByRole('region', { name: 'Conversations' });
  await list
    .getByRole('listitem')
    .filter({ hasText: '(425) 555-0199' })
    .getByRole('button')
    .first()
    .click();
  const pane = page.getByRole('region', { name: '(425) 555-0199' });
  await expect(
    pane.locator('[data-message-id="bounded-399"][data-message-part="text"]'),
  ).toHaveCount(1);
  return pane;
}

function visibleParts(element: Element) {
  const top = element.getBoundingClientRect().top;
  const bottom = top + element.clientHeight;
  return [...element.querySelectorAll<HTMLElement>('[data-message-id][data-message-part]')]
    .filter(
      (part) =>
        part.getBoundingClientRect().bottom > top && part.getBoundingClientRect().top < bottom,
    )
    .map((part) => ({
      id: part.dataset.messageId!,
      part: part.dataset.messagePart!,
      top: part.getBoundingClientRect().top - top,
    }));
}

for (const status of ['waiting', 'delivered'] as const) {
  test(`a focused failed Retry acknowledges ${status} before any replacement read can settle it`, async ({
    page,
  }) => {
    await login(page);
    await openDsp(page, 'Northline Logistics');
    const all = textHistory();
    all[399] = { ...all[399]!, direction: 'out', status: 'failed', error: 'phones_offline' };
    const { stop } = await textFixture(page, all, true);
    const entered = eventGate();
    const acknowledge = eventGate();
    await page.route('**/messages/bounded-399/retry', async (route) => {
      entered.release();
      await acknowledge.promise;
      all[399] = { ...all[399]!, status, error: null };
      await route.fulfill({ json: all[399] });
    });
    try {
      const pane = await openText(page);
      await pane.getByLabel('Message', { exact: true }).fill('Retry preserves the draft');
      const retry = pane.getByRole('button', { name: 'Retry', exact: true });
      await retry.click();
      await entered.promise;
      await expect(retry).toBeFocused();
      // No blur, focus transfer or second invalidation before acknowledgement.
      acknowledge.release();
      await expect(
        pane.getByText(status === 'waiting' ? 'Sending' : 'Delivered', { exact: true }),
      ).toBeVisible();
      await expect(pane.getByText('Not sent', { exact: true })).toHaveCount(0);
      await expect(retry).toHaveCount(0);
      await expect(pane.getByLabel('Message', { exact: true })).toHaveValue(
        'Retry preserves the draft',
      );
    } finally {
      acknowledge.release();
      await stop();
    }
  });
}

test('a selection made before readiness defers one update and its release resumes that update without a second invalidation', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const all = textHistory();
  const { invalidate, stop } = await textFixture(page, all);
  try {
    const pane = await openText(page);
    const thread = pane.locator('.thread');
    await thread
      .locator('[data-message-id="bounded-399"][data-message-part="text"]')
      .evaluate((element) => {
        const range = document.createRange();
        range.selectNodeContents(element);
        const selection = getSelection()!;
        selection.removeAllRanges();
        selection.addRange(range);
      });
    all[399] = { ...all[399]!, text: 'The deferred update arrived' };
    await invalidate();
    await expect(thread).not.toHaveClass(/windowed/);
    await page.evaluate(() => getSelection()!.removeAllRanges());
    await expect(thread.getByText('The deferred update arrived', { exact: true })).toBeVisible();
  } finally {
    await stop();
  }
});

test('legacy400 first-visible250 retains its complete visible suffix and offset during readiness conversion', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const all = textHistory();
  const { fixture, invalidate, stop } = await textFixture(page, all);
  try {
    const pane = await openText(page);
    const thread = pane.locator('.thread');
    for (let first = 200; first >= 0; first -= 50) {
      await thread.evaluate((element) => {
        element.scrollTop = 0;
      });
      await expect(
        thread.locator(`[data-message-id="bounded-${first}"][data-message-part="text"]`),
      ).toHaveCount(1);
    }
    await expect(thread.locator('.bubble[data-message-id]')).toHaveCount(400);
    await pane.getByLabel('Message', { exact: true }).fill('Conversion preserves the draft');
    await thread.evaluate((element) => {
      const row = element.querySelector(
        '[data-message-id="bounded-250"][data-message-part="text"]',
      )!;
      element.scrollTop += row.getBoundingClientRect().top - element.getBoundingClientRect().top;
      element.dispatchEvent(new Event('scroll'));
    });
    const before = await thread.evaluate(visibleParts);
    expect(before[0]!.id).toBe('bounded-250');
    expect(new Set(before.map((part) => part.id)).size).toBeGreaterThan(1);
    fixture.ready = true;
    await invalidate();
    await expect(thread).toHaveClass(/windowed/);
    const suffix =
      Math.max(...before.map((part) => Number(part.id.slice('bounded-'.length)))) - 250;
    expect(Number(fixture.windows.at(-1)!.searchParams.get('leading'))).toBeLessThanOrEqual(
      149 - suffix,
    );
    await expect(thread.locator('.bubble[data-message-id]')).toHaveCount(150);
    for (const part of before) {
      const retained = thread.locator(
        `[data-message-id="${part.id}"][data-message-part="${part.part}"]`,
      );
      await expect(retained).toHaveCount(1);
    }
    const anchor = thread.locator(
      `[data-message-id="${before[0]!.id}"][data-message-part="${before[0]!.part}"]`,
    );
    await expect
      .poll(() =>
        anchor.evaluate(
          (part) =>
            part.getBoundingClientRect().top - part.closest('.thread')!.getBoundingClientRect().top,
        ),
      )
      .toBeCloseTo(before[0]!.top, 0);
    await expect(pane.getByLabel('Message', { exact: true })).toHaveValue(
      'Conversion preserves the draft',
    );
  } finally {
    await stop();
  }
});

test('a legacy photo viewer retains its opener while an already-dispatched readiness response waits, then releases conversion after focus leaves', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const all = textHistory(250);
  const { fixture, invalidate, stop } = await textFixture(page, all);
  const entered = eventGate();
  const answer = eventGate();
  try {
    const pane = await openText(page);
    const thread = pane.locator('.thread');
    const photo = pane.getByRole('button', { name: 'door.png', exact: true });
    await thread.locator('[data-message-part="file:bounded-photo"]').scrollIntoViewIfNeeded();
    await expect(photo).toBeEnabled();
    await pane.getByLabel('Message', { exact: true }).focus();
    fixture.ready = true;
    fixture.beforeWindow = async () => {
      entered.release();
      await answer.promise;
    };
    await invalidate();
    await entered.promise;
    await photo.click();
    const opener = await photo.elementHandle();
    const viewer = page.getByRole('dialog', { name: 'door.png' });
    await expect(viewer.getByRole('img', { name: 'door.png' })).toBeVisible();
    const response = page.waitForResponse(
      (response) => response.url().includes('/window?') && response.url().includes('anchor='),
    );
    answer.release();
    await (await response).finished();
    await page.evaluate(
      () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())),
    );
    await expect(thread).not.toHaveClass(/windowed/);
    await page.keyboard.press('Escape');
    await expect(photo).toBeFocused();
    expect(
      await opener!.evaluate(
        (element) => element.isConnected && document.activeElement === element,
      ),
    ).toBe(true);
    await pane.getByLabel('Message', { exact: true }).focus();
    await expect(thread).toHaveClass(/windowed/);
  } finally {
    answer.release();
    await stop();
  }
});

test('legacy on-phone video fetch and download own conversion until both operations release, even when disabling Play releases focus', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  expect(
    await page.evaluate(() => document.createElement('video').canPlayType('video/x-msvideo')),
  ).toBe('');
  const all = textHistory();
  all[399] = {
    ...all[399]!,
    files: [
      {
        id: 'bounded-video',
        name: 'route.avi',
        mime: 'video/x-msvideo',
        size: PIXEL.length,
        status: 'on_phone',
        version: null,
        preview: false,
      },
    ],
  };
  const { fixture, invalidate, stop } = await textFixture(page, all);
  const fetching = eventGate();
  const fetched = eventGate();
  const saving = eventGate();
  const saved = eventGate();
  await page.route('**/texts/files/bounded-video/fetch', async (route) => {
    fetching.release();
    await fetched.promise;
    await route.fulfill({ json: { kept: true } });
  });
  await page.route('**/texts/files/bounded-video', async (route) => {
    saving.release();
    await saved.promise;
    await route.fulfill({ contentType: 'video/x-msvideo', body: PIXEL });
  });
  try {
    const pane = await openText(page);
    const thread = pane.locator('.thread');
    const play = pane.getByRole('button', { name: 'Play route.avi' });
    await play.click();
    await fetching.promise;
    await expect(play).toBeDisabled();
    fixture.ready = true;
    await invalidate();
    await expect(thread).not.toHaveClass(/windowed/);
    fetched.release();
    await saving.promise;
    await expect(thread).not.toHaveClass(/windowed/);
    const download = page.waitForEvent('download');
    saved.release();
    await download;
    await expect(thread).toHaveClass(/windowed/);
    await expect(pane.getByRole('alert')).toHaveCount(0);
  } finally {
    fetched.release();
    saved.release();
    await stop();
  }
});

for (const outside of [false, true]) {
  test(
    'a legacy fallback rechecks ' +
      (outside ? 'outside-anchored' : 'inside') +
      ' selection after its extra await and resumes on release without another invalidation',
    async ({ page }) => {
      await login(page);
      await openDsp(page, 'Northline Logistics');
      const all = textHistory();
      const { fixture, invalidate, stop } = await textFixture(page, all);
      const entered = eventGate();
      const answer = eventGate();
      try {
        const pane = await openText(page);
        const thread = pane.locator('.thread');
        await thread.evaluate((element) => {
          const row = element.querySelector(
            '[data-message-id="bounded-300"][data-message-part="text"]',
          )!;
          element.scrollTop +=
            row.getBoundingClientRect().top - element.getBoundingClientRect().top;
          element.dispatchEvent(new Event('scroll'));
        });
        await pane.getByLabel('Message', { exact: true }).focus();
        fixture.beforeLegacy = async () => {
          entered.release();
          await answer.promise;
        };
        all[399] = { ...all[399]!, text: 'Fallback published after release' };
        await invalidate();
        await entered.promise;
        await pane.evaluate((element, outside) => {
          const row = element.querySelector(
            '[data-message-id="bounded-300"][data-message-part="text"]',
          )!;
          const range = document.createRange();
          range.selectNodeContents(row);
          if (outside) range.setStart(element.querySelector('.thread-head .who')!, 0);
          const selection = getSelection()!;
          selection.removeAllRanges();
          selection.addRange(range);
        }, outside);
        expect(
          await thread.evaluate((element) => element.contains(getSelection()!.anchorNode)),
        ).toBe(!outside);
        const response = page.waitForResponse((response) =>
          /\/conversations\/[^/?]+$/.test(new URL(response.url()).pathname),
        );
        answer.release();
        await (await response).finished();
        await page.evaluate(
          () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())),
        );
        await expect(
          thread.getByText('Fallback published after release', { exact: true }),
        ).toHaveCount(0);
        await page.evaluate(() => getSelection()!.removeAllRanges());
        await expect(
          thread.getByText('Fallback published after release', { exact: true }),
        ).toHaveCount(1);
      } finally {
        answer.release();
        await stop();
      }
    },
  );
}
