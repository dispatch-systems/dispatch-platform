import fs from 'node:fs';
import { test, expect } from '../../../core/shell/tests/support/fixtures.js';
import { exports, item, open } from '../../../core/platform_owner/tests/support/audit-log.js';

// The audit log in the product's own words: each feature's and collector's wording, and the
// catalog's permission and connection names, as the built dashboard installs them.

test('the audit log reads as sentences, shows what changed and folds repeated visits', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const requests = await open(page);
  await expect(page.getByRole('heading', { name: /Sep 16/ })).toBeVisible();
  await expect(page.getByRole('heading', { name: /Sep 15/ })).toBeVisible();

  // Areas with activity are offered in the platform log.
  await expect(page.getByRole('group', { name: 'Area' }).getByRole('button')).toHaveText([
    /^All/,
    /^Team/,
    /^Roles/,
    /^Collections/,
    /^Schedules/,
    /^Connections/,
    /^Settings/,
    /^Failures/,
  ]);
  await expect(page.getByPlaceholder('Search people, roles, schedules…')).toBeVisible();
  await expect(page.getByLabel('DSP', { exact: true })).toBeVisible();

  const completed = item(page, 'Paycom collection for Sep 15 completed');
  await expect(completed).toContainText('Requested by Maria Lopez·1m 48s');
  await expect(item(page, 'started a Paycom collection')).toContainText('for Sep 15');
  await expect(item(page, 'Scheduled collection Morning Paycom pull failed')).toContainText(
    'Paycom needs verification — sign-in was challenged',
  );
  await expect(item(page, 'Sam Rivera joined the team')).toContainText(
    'Invited by Maria Lopez·Dispatcher',
  );
  const role = item(page, 'changed Jordan Pike’s role');
  await expect(role).toContainText('Dispatcher');
  await expect(role).toContainText('Manager');
  await expect(role).toContainText('3:50 PM');
  await expect(item(page, 'invited sam@northline.test')).toContainText('Dispatcher');
  await expect(item(page, 'updated the schedule Morning pull')).toContainText('5:30 AM');
  await expect(item(page, 'updated the role Dispatcher')).toContainText('+ Manage Timecard');
  await expect(item(page, 'updated the role Dispatcher')).toContainText('− Invite Members');

  await expect(role.locator('.audit-icon')).toHaveCSS('border-top-width', '0px');
  const visit = item(page, 'Sam Rivera opened this DSP');
  await expect(visit.locator('.audit-icon')).toHaveCSS('border-top-width', '0px');
  await expect(visit).not.toHaveClass(/quiet/);
  await expect(visit.locator('strong', { hasText: 'Sam Rivera' })).toBeVisible();
  await expect(item(page, 'Platform Owner updated DSP settings')).not.toHaveClass(/quiet/);
  const visits = item(page, 'Platform Owner opened Northline Logistics 3 times');
  await expect(visits).toHaveClass(/quiet/);
  await expect(visits).toHaveCount(1);
  await expect(visits).toContainText('11:01 AM – 12:02 PM');
  await visits.getByRole('button').click();
  await expect(visits).toContainText('12:02 PM, 11:40 AM, 11:01 AM');

  await role.getByRole('button').click();
  await expect(role).toContainText('member.role_changed · #96');
  await expect(role).toContainText(/Tue, Sep 15, 2026.*3:50:00 PM/);
  await expect(item(page, 'completed the DSP profile')).toContainText(
    'StationTST2·AbbreviationNLOG',
  );

  await expect(page.getByText('Showing 14 of 24')).toBeVisible();
  await page.getByRole('button', { name: 'Load more', exact: true }).click();
  await expect.poll(() => requests.at(-1)?.get('limit')).toBe('100');
  // Older events fall back to the wording their data supports.
  await expect(item(page, 'changed a member’s role')).toContainText('Member');
  await expect(item(page, 'Collection completed')).toBeVisible();
  await expect(item(page, 'deleted a schedule')).toBeVisible();
  await expect(item(page, 'Maria Lopez disconnected Cortex')).toBeVisible();
  await expect(item(page, 'updated employee links').first()).toContainText('2 links changed');
  await expect(item(page, 'updated employee links').last()).toContainText(
    '2 linked·1 kept separate',
  );
  const paycom = item(page, 'updated Paycom settings');
  await expect(paycom).toContainText('Automatic syncOnOff');
  await expect(paycom).toContainText('Late DA time10:01 AM9:45 AM');
  await expect(paycom).toContainText('DepartmentAllDrivers');
  await expect(item(page, 'DVIC collection attempt 1 of 3 failed')).toContainText(
    'Cortex took too long to respond·Northline Logistics·Retrying',
  );
  await expect(item(page, /^Meal break collection failed/)).toContainText('After 3 attempts');
  await expect(item(page, 'Maria Lopez vehicle inspection logged')).toContainText('Van 12');
  await expect(page.getByText('schedule_0123', { exact: false })).toHaveCount(0);

  // From one event to everything about its subject, and back.
  await role.getByRole('button', { name: 'All activity involving Jordan Pike' }).click();
  await expect.poll(() => requests.at(-1)?.get('subject')).toBe('member:usr_jordan');
  expect(requests.at(-1)?.get('named')).toBe('Jordan Pike');
  await expect(page.getByRole('listitem')).toHaveCount(1);
  await page.getByRole('button', { name: 'Stop showing only Jordan Pike' }).click();
  await expect.poll(() => requests.at(-1)?.has('subject')).toBe(false);
  await expect(item(page, 'Platform Owner opened Northline Logistics')).toBeVisible();

  await page.getByRole('button', { name: /^Failures/ }).click();
  await expect(page.getByRole('button', { name: /^Failures/ })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.getByRole('listitem')).toHaveCount(2);
  expect(requests.at(-1)?.get('area')).toBe('failures');
  await page.getByRole('button', { name: /^All/ }).click();

  await page.getByLabel('Person').selectOption({ label: 'Maria Lopez' });
  await expect.poll(() => requests.at(-1)?.get('actor')).toBe('usr_maria');
  await page.getByLabel('Search activity').fill('role');
  await expect.poll(() => requests.at(-1)?.get('q')).toBe('role');
  expect(requests.at(-1)?.get('from')).toBeTruthy();
  await page.getByLabel('Date range').selectOption({ label: 'All time' });
  await expect.poll(() => requests.at(-1)?.has('from')).toBe(false);

  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export', exact: true }).click();
  const csv = fs.readFileSync(await (await download).path(), 'utf8');
  // The export carries the filters in force, not a page of them.
  expect(exports).toHaveLength(1);
  expect(exports[0]!.get('actor')).toBe('usr_maria');
  expect(exports[0]!.get('q')).toBe('role');
  expect(csv).toContain('"Time","Person","DSP","Area","Event","Details","Action"');
  expect(csv).toContain('"Maria Lopez changed Jordan Pike’s role","Role Dispatcher → Manager"');
  expect(errors).toEqual([]);
});
