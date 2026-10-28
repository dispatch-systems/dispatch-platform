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
// DSP keeps a zone of its own, chosen so its reading window is open whatever the hour the test
// runs, with its day under way.

/** The earliest the DSP's time may be as a test starts: the demo phone's text from 0190 is an
 * hour old, so it's of today, the day being read, with half an hour to spare. */
const EARLIEST = '01:30';
/** The latest the DSP's time may be as a test starts: its reading window, open until 23:59, stays
 * open for the test with an hour to spare. */
const LATEST = '23:00';

/** A picture of a single pixel, as a PNG. */
const PIXEL = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==',
  'base64',
);

/** The time it is `hours` east of UTC, as Settings writes a time. */
const timeAt = (hours: number) =>
  new Date(Date.now() + hours * 3_600_000).toISOString().slice(11, 16);
/** Hours east of UTC where the time is from `EARLIEST` until `LATEST`. */
function underWay() {
  for (let hours = -12; hours <= 14; hours++) {
    const time = timeAt(hours);
    if (time >= EARLIEST && time < LATEST) return hours;
  }
  throw new Error('No zone fits now');
}

/**
 * Northline reading its routes live, `hours` east of UTC, once its day's drivers are read; the
 * address of its Communications page, and ending its day, as its reading window closing does:
 * the day saved and live route data emptied.
 */
async function reading(dispatch: Dispatch, hours: number) {
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  const moved = await owner.post('/api/dsp/profile', {
    name: north.name,
    abbreviation: 'NLL',
    stationCode: 'TST1',
    timezone: `Etc/GMT${hours > 0 ? '-' : '+'}${Math.abs(hours)}`,
  });
  expect(moved.status, moved.body).toBe(200);
  await owner.select(north.id);
  // Its window, saved, which wakes the readers: a window open now reads at once, one closed ends
  // the day.
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
  const connected = await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  expect(connected.value.status, connected.body).toBe('ready');
  // A snapshot of today names the scope its reads are in, and its job's end closes the browser
  // signing in opened, which a live read can't share.
  const queued = await owner.post('/api/dsp/routes/collect', {
    requestId: 'scope',
    date: new Date(Date.now() + hours * 3_600_000).toISOString().slice(0, 10),
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
  return {
    page: `/#dsp/${north.id}/communications`,
    end: () => window('00:00', timeAt(hours)),
    /** Texts one of today's drivers, as someone at the DSP does. */
    text: async (driverId: string, text: string) => {
      const opened = await owner.post('/api/dsp/communications/texts/conversations', { driverId });
      expect(opened.status, opened.body).toBe(200);
      const sent = await owner.post(
        `/api/dsp/communications/conversations/${opened.value.id}/messages`,
        { text, photos: [] },
      );
      expect(sent.status, sent.body).toBe(201);
    },
  };
}

test('Today’s Routes lists the day’s drivers on the board and in the list, and opens a driver’s conversation from each', async ({
  page,
  dispatch,
}) => {
  const northline = await reading(dispatch, underWay());
  await login(page);
  await page.goto(northline.page);

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
  const northline = await reading(dispatch, underWay());
  await login(page);
  // Today's Routes as read while the day is on, which answers its read again once it's over: taken
  // from its first read as the test fetches it for the page, before the page has it, so opening a
  // conversation, which replaces the page's address and with it what the browser kept of the
  // read, can't come first.
  const routesRead = '**/api/dsp/communications/routes';
  let duringDay!: (routes: unknown) => void;
  const during = new Promise((resolve) => (duringDay = resolve));
  await page.route(
    routesRead,
    async (route) => {
      const response = await route.fetch();
      const read = await response.json();
      duringDay(read);
      await route.fulfill({ response, json: read });
    },
    { times: 1 },
  );
  await page.goto(northline.page);
  const routes = await during;
  await page
    .getByRole('region', { name: 'Conversations' })
    .getByRole('button', { name: /Fixture Driver/ })
    .click();
  const conversation = page.getByRole('region', { name: 'Fixture Driver' });

  // A message of a few lines and a picture, written and not sent.
  const box = conversation.getByRole('textbox', { name: 'Message' });
  const draft = 'Gate code is 4471\nSide door, by the dock\nCall if it sticks';
  await box.fill(draft);
  const height = await box.evaluate((field) => field.getBoundingClientRect().height);
  const choosing = page.waitForEvent('filechooser');
  await conversation.getByRole('button', { name: 'Attach photos' }).click();
  await (await choosing).setFiles({ name: 'gate.png', mimeType: 'image/png', buffer: PIXEL });
  const picture = conversation.getByRole('img', { name: 'gate.png' });
  await expect(picture).toBeVisible();

  // Their profile shows the work phone they hold, as their conversation's head does.
  await conversation.getByRole('button', { name: /^Fixture Driver/ }).click();
  const profile = page.getByRole('dialog', { name: 'Fixture Driver' });
  await expect(profile.getByText('Work phone')).toBeVisible();

  // The day ends while Today's Routes can't be read: their conversation, read again, says they
  // hold no work phone, as the profile left open does, though Today's Routes' last read, older,
  // had one.
  await page.route(routesRead, (route) =>
    route.fulfill({ status: 500, json: { error: 'internal', message: 'Something went wrong.' } }),
  );
  await northline.end();
  await expect(conversation.getByText('No work phone')).toBeVisible();
  await expect(profile.getByText('Work phone')).toHaveCount(0);
  await page.keyboard.press('Escape');
  // Read again, as the page comes back into view, Today's Routes says the day is over.
  await page.unroute(routesRead);
  await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await expect(page.getByText('No routes', { exact: true })).toBeVisible();
  await expect(conversation.getByText('No work phone')).toBeVisible();

  // Today's Routes reads them holding their phone again, as the next day's first read would (a
  // day can't begin here without moving the DSP's zone, which opens its view afresh): what was
  // written is still there, as high as it was written.
  await page.route(routesRead, (route) => route.fulfill({ json: routes }));
  await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await expect(box).toHaveValue(draft);
  await expect(picture).toBeVisible();
  await expect
    .poll(() => box.evaluate((field) => field.getBoundingClientRect().height))
    .toBe(height);
});

test('a driver’s progress stays as it was last read, though another driver’s change finds Today’s Routes unreadable', async ({
  page,
  dispatch,
}) => {
  const northline = await reading(dispatch, underWay());
  await login(page);
  await page.goto(northline.page);
  const list = page.getByRole('region', { name: 'Conversations' });
  await list.getByRole('button', { name: /Fixture Driver/ }).click();
  const conversation = page.getByRole('region', { name: 'Fixture Driver' });
  await conversation.getByRole('button', { name: /^Fixture Driver/ }).click();
  const profile = page.getByRole('dialog', { name: 'Fixture Driver' });
  // As their conversation was read as it opened: the fixture's two stops, both done.
  await expect(profile.getByText('2/2')).toBeVisible();

  // Today's Routes read after it, as the page comes back into view, has them further on.
  const routesRead = '**/api/dsp/communications/routes';
  await page.route(routesRead, async (route) => {
    const response = await route.fetch();
    const read = await response.json();
    const further = { stops: 168, stopsDone: 98, pace: null };
    read.drivers = read.drivers.map((each: { driverId: string; today: object }) =>
      each.driverId === 'driver-1' ? { ...each, today: { ...each.today, ...further } } : each,
    );
    await route.fulfill({ response, json: read });
  });
  await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await expect(profile.getByText('98/168')).toBeVisible();

  // Second Driver is texted, which reads Today's Routes again, and that read fails: Fixture
  // Driver's conversation, not read again and so older, doesn't take their progress back.
  await page.unroute(routesRead);
  await page.route(routesRead, (route) =>
    route.fulfill({ status: 500, json: { error: 'internal', message: 'Something went wrong.' } }),
  );
  await northline.text('driver-2', 'Running 20 late');
  await expect(list.getByRole('alert')).toHaveText('Something went wrong.');
  await expect(profile.getByText('98/168')).toBeVisible();
});
