import {
  test,
  expect,
  login,
  openDsp,
} from '../../../../../../core/shell/tests/support/fixtures.js';
import { hrLine, paired, texting, withHrLine } from '../support/line.js';

// Northline, given HR Line's custom ID with HR Line on and a phone paired to it, its texts filed;
// its demo's stand-in phone, (206) 555-0100, is Dispatch Phone's, which (425) 555-0199 texted
// already.

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
  texting(dispatch, north.id, phone);
  await phone.texted('4255550199', 'About my paycheck');
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

});
