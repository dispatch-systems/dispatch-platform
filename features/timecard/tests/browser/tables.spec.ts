import fs from 'node:fs';
import { test, expect } from '../../../../core/shell/tests/support/fixtures.js';
import { openAuthenticatedDsp } from '../support/page.js';

test('timecards export every column and row in the order shown', async ({ page, dispatch }) => {
  await openAuthenticatedDsp(page, dispatch, 'Northline Logistics');
  await expect(page.getByRole('button', { name: /View punches for/ })).toHaveCount(12);
  // Export a changed order, so exporting unsorted backing rows fails.
  await page.getByRole('button', { name: 'Hours', exact: true }).first().click();
  await page.getByRole('button', { name: 'Hours', exact: true }).first().click();
  const shown = await page
    .locator('.paycom-timecard-table tbody tr')
    .evaluateAll((rows) =>
      rows.map((row) => [...row.querySelectorAll('td')].map((cell) => cell.textContent!.trim())),
    );
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export timecards', exact: true }).click();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(/^timecards-\d{4}-\d{2}-\d{2}\.csv$/);
  const lines = fs.readFileSync(await file.path(), 'utf8').split('\r\n');
  expect(lines[0]).toBe(
    '\uFEFF"Employee","Clock in","Lunch out","Lunch in","Clock out","Hours","Punch status"',
  );
  expect(lines).toHaveLength(13);
  const exported = lines
    .slice(1)
    .map((line) =>
      [...line.matchAll(/"((?:[^"]|"")*)"(?:,|$)/g)].map((cell) => cell[1]!.replaceAll('""', '"')),
    );
  expect(exported).toEqual(shown);
  await expect(page.getByLabel('Choose columns')).toHaveCount(0);
});

test('a second lunch stacks in its cells without widening them', async ({ page, dispatch }) => {
  let name = '';
  // The day the page opens on, the first asked for: today, which the demo data always has. The
  // days beside it, which the page warms, may have none, as on the first day of a pay period.
  let opened: string | null | undefined;
  await page.route('**/api/dsp/timecards?*', async (route) => {
    const day = new URL(route.request().url()).searchParams.get('date');
    opened ??= day;
    if (day !== opened) return route.fallback();
    const response = await route.fetch();
    const body = await response.json();
    name = body.rows[0].name;
    body.rows[0].punches = [
      { in: '08:00', out: '12:00', hours: 4 },
      { in: '12:30', out: '14:00', hours: 1.5 },
      { in: '14:30', out: '16:30', hours: 2 },
    ];
    await route.fulfill({ response, json: body });
  });
  await openAuthenticatedDsp(page, dispatch, 'Northline Logistics');
  await expect(page.getByRole('button', { name: /View punches for/ })).toHaveCount(12);
  const row = page.locator('.paycom-timecard-table tbody tr', { hasText: name });
  await expect(row.locator('td').nth(2).locator('.punch-times > span')).toHaveText([
    '12:00',
    '14:00',
  ]);
  await expect(row.locator('td').nth(3).locator('.punch-times > span')).toHaveText([
    '12:30',
    '14:30',
  ]);
  // Clock in, both lunches and Clock out keep one shared width.
  const widths = await page
    .locator('.paycom-timecard-table thead tr')
    .first()
    .locator('th')
    .evaluateAll((cells) =>
      cells.slice(1, 5).map((cell) => Math.round(cell.getBoundingClientRect().width)),
    );
  expect(new Set(widths).size).toBe(1);
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export timecards', exact: true }).click();
  const csv = fs.readFileSync(await (await download).path(), 'utf8');
  expect(csv).toContain('"08:00","12:00, 14:00","12:30, 14:30","16:30"');
});

test('arrow keys, j and k move between the rows of a table', async ({ page, dispatch }) => {
  await openAuthenticatedDsp(page, dispatch, 'Northline Logistics');
  const rows = page.getByRole('button', { name: /View punches for/ });
  await expect(rows).toHaveCount(12);
  await rows.first().focus();
  await page.keyboard.press('ArrowDown');
  await expect(rows.nth(1)).toBeFocused();
  await page.keyboard.press('j');
  await expect(rows.nth(2)).toBeFocused();
  await page.keyboard.press('k');
  await page.keyboard.press('ArrowUp');
  await expect(rows.first()).toBeFocused();
  // The first row has nowhere further up to go.
  await page.keyboard.press('ArrowUp');
  await expect(rows.first()).toBeFocused();
});

test('meal break details span the table and the export splits each source', async ({
  page,
  dispatch,
}) => {
  await openAuthenticatedDsp(page, dispatch, 'Northline Logistics');
  await page.getByRole('tab', { name: 'Meal Breaks', exact: true }).click();
  const table = page.locator('.meal-table');
  await expect(table.getByRole('columnheader')).toHaveCount(8);
  await table
    .getByRole('button', { name: /^Details for / })
    .first()
    .click();
  await expect(table.locator('.meal-detail > td')).toHaveAttribute('colspan', '8');

  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export meal breaks', exact: true }).click();
  const csv = fs.readFileSync(await (await download).path(), 'utf8');
  expect(csv.split('\r\n')[0]).toBe(
    '\uFEFF"Employee","IN DAY","Last delivery","OUT LUNCH Paycom","OUT LUNCH Flex","IN LUNCH Paycom","IN LUNCH Flex","First delivery","OUT DAY","Comparison"',
  );
});
