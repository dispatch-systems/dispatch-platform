import {
  test,
  expect,
  login,
  openDsp,
} from '../../../../../../core/shell/tests/support/fixtures.js';
import { hrLine, paired, withHrLine } from '../support/line.js';

// Northline, given HR Line's custom ID with HR Line on and a phone paired to it; its demo's
// stand-in phone, (206) 555-0100, is Dispatch Phone's, which (425) 555-0199 texted already.

/** A picture of a single pixel, as a PNG. */
const PIXEL = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==',
  'base64',
);

test("HR Line's page lists, names and answers HR Line's texts alone, and Dispatch Phone's page none of them", async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = await withHrLine(owner);
  const phone = await paired(dispatch, owner, hrLine, '(206) 555-0111');
  await phone.texted('4255550199', 'About my paycheck');
  // Dispatch Phone's quick replies, which HR Line's page and its Display section's preview lack.
  const saved = await owner.post('/api/dsp/dispatch-phone/quick-replies', {
    replies: ['On my way'],
    revision: 0,
  });
  expect(saved.status).toBe(200);
  await login(page);
  await openDsp(page, 'Northline Logistics');
  // Its own count beside it in the sidebar.
  const link = page.getByRole('link', { name: /^HR Line/ });
  await expect(link.getByLabel('1 waiting')).toBeVisible();
  await link.click();
  await expect(page.getByRole('heading', { name: 'HR Line', level: 1 })).toBeVisible();
  const list = page.getByRole('region', { name: 'Conversations' });
  // A group is Dispatch Phone's: an address naming one opens none here, nor reads one.
  const groups: string[] = [];
  page.on('request', (request) => {
    if (request.url().includes('/groups')) groups.push(request.url());
  });
  const origin = new URL(page.url()).origin;
  await page.goto(`${origin}/#dsp/${north.id}/cjf87gqd-hr-line?group=grp_${'0'.repeat(32)}`);
  await page.reload();
  await expect(list.getByRole('button', { name: /About my paycheck/ })).toBeVisible();
  await expect(page.getByRole('region', { name: '(425) 555-0199', exact: true })).toBeVisible();
  expect(groups).toEqual([]);
  await list.getByRole('button', { name: /\(425\) 555-0199.*About my paycheck/ }).click();

  // A name alone is saved for the number, HR Line's own.
  const conversation = page.getByRole('region', { name: '(425) 555-0199', exact: true });
  await conversation.getByRole('button', { name: 'Save contact' }).click();
  await expect(page.getByRole('tab', { name: 'Add to driver' })).toHaveCount(0);
  await page.getByLabel('Name', { exact: true }).fill('Jordan Ellis');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  const named = page.getByRole('region', { name: 'Jordan Ellis', exact: true });
  await expect(named).toBeVisible();
  await expect(named.getByRole('button', { name: 'Quick replies' })).toHaveCount(0);

  // Answered with a picture, from HR Line's phone.
  await named.locator('input[type=file]').setInputFiles({
    name: 'stub.png',
    mimeType: 'image/png',
    buffer: PIXEL,
  });
  await named.getByLabel('Message', { exact: true }).fill('Payroll will call you');
  await named.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(named.getByText('Payroll will call you', { exact: true })).toBeVisible();
  await expect(named.getByRole('img', { name: 'stub.png' })).toBeVisible();

  // Dispatch Phone's page has the same number's own conversation, as it was.
  await page.getByRole('link', { name: /^Dispatch Phone/ }).click();
  const theirs = page
    .getByRole('region', { name: 'Conversations' })
    .getByRole('listitem')
    .filter({ hasText: '(425) 555-0199' });
  await expect(theirs).toContainText('Hi is this dispatch? I start Monday');
  await expect(page.getByText('About my paycheck')).toHaveCount(0);
  await expect(page.getByText('Payroll will call you')).toHaveCount(0);
  await expect(page.getByText('Jordan Ellis')).toHaveCount(0);

  // The Display page has one section for both lines, Dispatch Phone's, with no line's name over
  // it; what's chosen there is HR Line's page's too.
  await page.goto(`${origin}/#dsp/${north.id}/settings?tab=display`);
  const section = (id: string) => page.locator(`[data-settings-section="${id}"]`);
  const replies = (id: string) => section(id).locator('.quick-replies-button');
  await expect(replies('communications.display')).toHaveCount(1);
  await expect(section('cjf87gqd_hr_line.display')).toBeEmpty();
  await expect(page.getByRole('heading', { name: 'Dispatch Phone', level: 2 })).toHaveCount(0);
  const chosen = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/dsp/settings/communications.display') && response.ok(),
  );
  await page.getByRole('radiogroup', { name: 'Layout' }).getByText('Full width').click();
  await chosen;
  await page.goto(`${origin}/#dsp/${north.id}/cjf87gqd-hr-line`);
  await expect(page.locator('.communications')).toHaveAttribute('style', /--column-width: 100%/);

  // With Dispatch Phone off, the page has HR Line's section, previewing its own page without
  // Dispatch Phone's quick replies, or its Previous days.
  const switches = `/api/platform/dsps/${north.id}/features`;
  const off = await owner.post(switches, { feature: 'communications', enabled: false });
  expect(off.status, off.body).toBe(200);
  await page.goto(`${origin}/#dsp/${north.id}/settings?tab=display`);
  await page.reload();
  await expect(section('cjf87gqd_hr_line.display').locator('.message-display')).toBeVisible();
  await expect(replies('cjf87gqd_hr_line.display')).toHaveCount(0);
  await expect(section('cjf87gqd_hr_line.display').locator('.previous-days')).toHaveCount(0);
});

/** When `now` was, in milliseconds, in `timeZone`'s day before it: between a day before it, as long
 * before pairing as a phone's texts are kept as written, and that day's end. */
function yesterday(now: number, timeZone: string) {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone,
    hour: 'numeric',
    minute: 'numeric',
    second: 'numeric',
    hourCycle: 'h23',
  }).formatToParts(now);
  const part = (type: string) => Number(parts.find((each) => each.type === type)!.value);
  const today =
    now - (part('hour') * 3600 + part('minute') * 60 + part('second')) * 1000 - (now % 1000);
  return Math.floor((today + now - 24 * 60 * 60 * 1000) / 2);
}

test("HR Line's page lists every day's conversations, with no Previous days", async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = (await withHrLine(owner)) as { id: string; timezone: string };
  const phone = await paired(dispatch, owner, hrLine, '(206) 555-0111');
  // Texted yesterday and marked done, as a conversation Dispatch Phone keeps under Previous days;
  // and one waiting today.
  await phone.texted('4255550199', 'About my paycheck', yesterday(Date.now(), north.timezone));
  const listed = async () =>
    ((await owner.read(hrLine)).conversations as { id: string }[]).map((each) => each.id);
  await expect.poll(listed).toHaveLength(1);
  const [id] = await listed();
  const done = await owner.post(`${hrLine}/conversations/${id}/done`, {});
  expect(done.status).toBe(200);
  await phone.texted('4255550123', 'Can I swap Friday?');
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: /^HR Line/ }).click();
  const list = page.getByRole('region', { name: 'Conversations' });
  await expect(list.getByRole('button', { name: /Can I swap Friday\?/ })).toBeVisible();
  await expect(list.getByRole('heading', { name: 'Yesterday' })).toBeVisible();
  await expect(list.getByRole('button', { name: /About my paycheck/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /previous days/i })).toHaveCount(0);
});
