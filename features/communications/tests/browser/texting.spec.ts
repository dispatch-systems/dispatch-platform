import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import { connect, event, fromSlack, install } from '../../../slack/tests/support/slack.js';

// The seeded DSP texts through its demo's stand-in phone, (206) 555-0100, which received three
// texts and delivers what it's given at once; it has no Slack until a test connects it. Routes is
// on, and Live Route Data, which reads a provider, off: Today's Routes lists nobody.

/** A picture of a single pixel, as a PNG. */
const PIXEL = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==',
  'base64',
);

test('with its phone and no Slack, Communications opens on Today’s Routes, with no routes before live route data reads today, beside Messaging: a number saved as a contact, asked before a name is left unsaved, answered by text with a picture, blocked into Contacts and unblocked', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  const sections = page.getByRole('tablist', { name: 'Sections' });
  await expect(sections.getByRole('tab', { name: "Today's Routes" })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(page.getByText('No routes', { exact: true })).toBeVisible();
  // Nothing of Slack.
  await expect(sections.getByRole('tab', { name: /Slack/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Connect Slack' })).toHaveCount(0);
  await sections.getByRole('tab', { name: /Messaging/ }).click();
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
  // A name written and left asks first: discarded, it's gone; saved, it's the contact's.
  await saveContact.click();
  await name.fill('Jamie Ortiz');
  await page.keyboard.press('Escape');
  await asking.getByRole('button', { name: 'Discard changes' }).click();
  await expect(name).toHaveCount(0);
  await expect(stranger).toBeVisible();
  await saveContact.click();
  await name.fill('Jamie Ortiz');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await asking.getByRole('button', { name: 'Save changes' }).click();
  await expect(list.getByRole('listitem').filter({ hasText: 'Jamie Ortiz' })).toBeVisible();

  // Answered by text, it goes from the DSP's phone, which delivers it.
  const jamie = page.getByRole('region', { name: 'Jamie Ortiz' });
  await jamie.getByLabel('Message').fill('See you Monday at 7');
  await jamie.getByLabel('Message').press('Enter');
  await expect(jamie.getByText('Delivered')).toBeVisible();
  // A picture goes by text too, kept by the phone, and shows as sent.
  const choosing = page.waitForEvent('filechooser');
  await jamie.getByRole('button', { name: 'Attach photos' }).click();
  await (await choosing).setFiles({ name: 'door.png', mimeType: 'image/png', buffer: PIXEL });
  await jamie.getByRole('button', { name: 'Send' }).click();
  await expect(jamie.getByRole('link', { name: 'door.png' })).toBeVisible();

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
  const list = page.getByRole('region', { name: 'Conversations' });
  await expect(list.getByRole('status')).toContainText('Loading');
  await expect(page.getByText('No routes', { exact: true })).toHaveCount(0);
  release();
  await expect(page.getByText('No routes', { exact: true })).toBeVisible();

  // Read again, as it is each time it shows, it fails: Today's Routes says so, Messaging doesn't.
  await page.unroute('**/api/dsp/communications/routes');
  await page.route('**/api/dsp/communications/routes', (route) =>
    route.fulfill({ status: 500, json: { error: 'internal', message: 'Something went wrong.' } }),
  );
  const sections = page.getByRole('tablist', { name: 'Sections' });
  await sections.getByRole('tab', { name: /Messaging/ }).click();
  await sections.getByRole('tab', { name: "Today's Routes" }).click();
  await expect(list.getByRole('alert')).toHaveText('Something went wrong.');
  await sections.getByRole('tab', { name: /Messaging/ }).click();
  await expect(list).toContainText('(425) 555-0199');
  await expect(page.getByRole('alert')).toHaveCount(0);

  // Opened again on Messaging, as chosen, then offline before Today's Routes was ever read: it says
  // it'll read on reconnecting, never No routes, and reads once back online.
  await page.unroute('**/api/dsp/communications/routes');
  await page.reload();
  await expect(list).toContainText('(425) 555-0199');
  await page.context().setOffline(true);
  await sections.getByRole('tab', { name: "Today's Routes" }).click();
  await expect(list.getByRole('status')).toContainText('offline');
  await expect(page.getByText('No routes', { exact: true })).toHaveCount(0);
  await page.context().setOffline(false);
  await expect(page.getByText('No routes', { exact: true })).toBeVisible();
});

test('with Slack too, the sections switch, each with how many wait, stay as chosen, and a link opens its section, the board chosen or not', async ({
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
  await messaging.click();
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
  const toText = `/#dsp/${north.id}/communications?open=${text.id}`;
  await page.goto(toText);
  await expect(page.getByRole('region', { name: '(425) 555-0199' })).toBeVisible();
  await expect(messaging).toHaveAttribute('aria-selected', 'true');

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
  await expect(messaging).toHaveAttribute('aria-selected', 'true');

  // A link to a group opens it in Slack, and one to the text again, Messaging.
  const group = await owner.post('/api/dsp/communications/groups', {
    name: 'Morning wave',
    people: [{ teamId: 'T0FIXTURE1', userId: 'U0FIXJORDAN' }],
    text: 'Running 20 minutes late',
    photos: [],
  });
  expect(group.status, group.body).toBe(201);
  await page.goto(`/#dsp/${north.id}/communications?group=${group.value.id}`);
  await expect(page.getByRole('region', { name: 'Morning wave' })).toBeVisible();
  await expect(slack).toHaveAttribute('aria-selected', 'true');
  await page.goto(toText);
  await expect(page.getByRole('region', { name: '(425) 555-0199' })).toBeVisible();
  await expect(messaging).toHaveAttribute('aria-selected', 'true');
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

test('with Slack needing an update, Messaging still works, and Update Slack covers Slack alone until closed', async ({
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
  await sections.getByRole('tab', { name: /Messaging/ }).click();
  const stranger = page
    .getByRole('region', { name: 'Conversations' })
    .getByRole('listitem')
    .filter({ hasText: '(425) 555-0199' });
  await expect(stranger).toBeVisible();
  await expect(update).toHaveCount(0);
  await stranger.getByRole('button').first().click();
  await expect(page.getByRole('region', { name: '(425) 555-0199' })).toBeVisible();

  // Slack waits on the update; closed, Messaging shows again.
  await sections.getByRole('tab', { name: /Slack/ }).click();
  await expect(update.getByRole('listitem')).toHaveText(/Operations/);
  await update.getByRole('button', { name: 'Close dialog' }).click();
  await expect(update).toHaveCount(0);
  await expect(sections.getByRole('tab', { name: /Messaging/ })).toHaveAttribute(
    'aria-selected',
    'true',
  );
});
