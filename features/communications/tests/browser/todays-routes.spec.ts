import {
  test,
  expect,
  login,
  type Dispatch,
} from '../../../../core/shell/tests/support/fixtures.js';

// Today's Routes filled as live route data fills it: Northline at a station, its Cortex connected
// in fixture mode, whose day has two drivers on work phones of their own, Fixture Driver on
// (206) 555-0190 and Second Driver on 0191, and Live Route Data reading all day. The DSP's demo
// phone received a text from 0190 an hour before, which files under the driver holding it. The
// DSP keeps a zone of its own, so its reading window is open whatever the hour the test runs, and
// a day can end and the next begin.

/** A picture of a single pixel, as a PNG. */
const PIXEL = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==',
  'base64',
);

/** The zone `hours` east of UTC, as the tz database names it. */
const zone = (hours: number) => `Etc/GMT${hours > 0 ? '-' : '+'}${Math.abs(hours)}`;
/** The day it is `hours` east of UTC now, and the time, as Settings writes a time. */
function clock(hours: number) {
  const local = new Date(Date.now() + hours * 3_600_000).toISOString();
  return { day: local.slice(0, 10), time: local.slice(11, 16) };
}
/**
 * Where the DSP is: in the west, its day under way, the text an hour old from today and its window
 * open for hours yet; in the east, a later day, mid-window.
 */
function zones() {
  for (const west of [-12, -11, -10])
    for (const east of [14, 13, 12]) {
      const [here, later] = [clock(west), clock(east)];
      if (
        here.time >= '01:30' &&
        here.time < '23:00' &&
        later.day > here.day &&
        later.time >= '00:05' &&
        later.time < '23:50'
      )
        return { west, east };
    }
  throw new Error('No zones fit now');
}

/** Northline reading its routes live, `hours` east of UTC, once its day's drivers are read. */
async function reading(dispatch: Dispatch, hours: number) {
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  const moveTo = async (hours: number) => {
    const moved = await owner.post('/api/dsp/profile', {
      name: north.name,
      abbreviation: 'NLL',
      stationCode: 'TST1',
      timezone: zone(hours),
    });
    expect(moved.status, moved.body).toBe(200);
    await owner.select(north.id);
  };
  // Its window, saved, which wakes the readers: a window open now reads at once, one ended stops
  // the day, saving it and emptying live route data.
  const window = async (start: string, stop: string) => {
    const section = 'live_route_data.collection';
    const settings = await owner.read('/api/dsp/settings');
    const { revision } = settings.sections.find((each: { id: string }) => each.id === section);
    const saved = await owner.post(`/api/dsp/settings/${section}`, {
      values: { start, stop },
      revision,
    });
    expect(saved.status, saved.body).toBe(200);
  };
  await moveTo(hours);
  const connected = await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  expect(connected.value.status, connected.body).toBe('ready');
  // A snapshot of today names the scope its reads are in, and its job's end closes the browser
  // signing in opened, which a live read can't share.
  const queued = await owner.post('/api/dsp/routes/collect', {
    requestId: 'scope',
    date: clock(hours).day,
    mode: 'snapshot',
  });
  expect(queued.status, queued.body).toBe(202);
  await expect
    .poll(async () => {
      const jobs: { id: string; status: string }[] = await owner.read('/api/dsp/routes/jobs');
      return jobs.find((job) => job.id === queued.value[0].id)?.status;
    })
    .toBe('succeeded');
  const switched = await owner.post(`/api/platform/dsps/${north.id}/features`, {
    feature: 'live_route_data',
    enabled: true,
  });
  expect(switched.status, switched.body).toBe(200);
  await owner.select(north.id);
  await window('00:00', '23:59');
  await expect
    .poll(async () => (await owner.read('/api/dsp/communications/routes')).drivers.length)
    .toBe(2);
  return { page: `/#dsp/${north.id}/communications`, moveTo, window };
}

test('Today’s Routes lists the day’s drivers on the board and in the list, and opens a driver’s conversation from each', async ({
  page,
  dispatch,
}) => {
  const read = await reading(dispatch, zones().west);
  await login(page);
  await page.goto(read.page);

  // The board: each driver on their route and work phone.
  await page.getByRole('button', { name: 'Board' }).click();
  const board = page.getByRole('region', { name: "Today's Routes" });
  const fixture = board.getByRole('row').filter({ hasText: 'Fixture Driver' });
  await expect(fixture).toContainText('CX101');
  await expect(fixture).toContainText('0190');
  await expect(board.getByRole('row').filter({ hasText: 'Second Driver' })).toContainText('0191');
  // Chosen there, a driver's conversation opens beside the list, with the text from their phone.
  await fixture.getByRole('button', { name: 'Fixture Driver' }).click();
  await expect(page.getByRole('region', { name: 'Fixture Driver' })).toContainText(
    'Done with lunch, back on route',
  );
  // From the list, a driver who hasn't texted: their conversation, started to text them first.
  await page
    .getByRole('region', { name: 'Conversations' })
    .getByRole('button', { name: /Second Driver/ })
    .click();
  await expect(
    page.getByRole('region', { name: 'Second Driver' }).getByRole('textbox', { name: 'Message' }),
  ).toHaveAttribute('placeholder', 'Message Second');
});

test('a driver’s work conversation keeps what’s being written while they hold no work phone, and their profile follows their day to its end', async ({
  page,
  dispatch,
}) => {
  const { west, east } = zones();
  const { page: communications, moveTo, window } = await reading(dispatch, west);
  await login(page);
  await page.goto(communications);
  await page
    .getByRole('region', { name: 'Conversations' })
    .getByRole('button', { name: /Fixture Driver/ })
    .click();
  const conversation = page.getByRole('region', { name: 'Fixture Driver' });

  // A message and a picture, written and not sent.
  const box = conversation.getByRole('textbox', { name: 'Message' });
  await box.fill('Gate code is 4471');
  const choosing = page.waitForEvent('filechooser');
  await conversation.getByRole('button', { name: 'Attach photos' }).click();
  await (await choosing).setFiles({ name: 'gate.png', mimeType: 'image/png', buffer: PIXEL });
  const picture = conversation.getByRole('img', { name: 'gate.png' });
  await expect(picture).toBeVisible();

  // Their profile shows the work phone they hold, as their conversation's head does.
  await conversation.getByRole('button', { name: /^Fixture Driver/ }).click();
  const profile = page.getByRole('dialog', { name: 'Fixture Driver' });
  await expect(profile.getByText('Work phone')).toBeVisible();

  // The day ends: they hold no work phone, as the profile says too, left open.
  await window('00:00', clock(west).time);
  await expect(conversation.getByText('No work phone')).toBeVisible();
  await expect(profile.getByText('Work phone')).toHaveCount(0);
  await page.keyboard.press('Escape');

  // The next day's first read hands them their phone again: what was written is still there.
  await moveTo(east);
  await window('00:00', '23:59');
  await expect(box).toHaveValue('Gate code is 4471');
  await expect(picture).toBeVisible();
});
