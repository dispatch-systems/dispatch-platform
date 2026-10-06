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
