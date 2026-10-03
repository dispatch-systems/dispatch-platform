import fs from 'node:fs';
import type { MealComparison } from '../../../shared/contracts/timecard.js';
import type { Membership } from '../../../shared/contracts/accounts.js';
import { test, expect, login, openDsp } from '../../../core/shell/tests/support/fixtures.js';

test('large meal rosters keep bounded rows, search responsive, and export every employee @paint-budget', async ({
  page,
}) => {
  let reads = 0;
  await page.route('**/api/dsp/paycom/meal-breaks?*', async (route) => {
    reads++;
    const response = await route.fetch();
    const data: MealComparison = await response.json();
    const sample = data.rows.find((row) => row.cortex.length) ?? data.rows[0]!;
    data.rows = Array.from({ length: 3000 }, (_, index) => ({
      ...sample,
      id: `render-${index}`,
      name: `Driver ${String(index).padStart(4, '0')}`,
    }));
    await route.fulfill({ response, json: data });
  });
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Timecard', exact: true }).click();
  await page.getByRole('tab', { name: 'Meal Breaks', exact: true }).click();
  const table = page.locator('.meal-table');
  await expect(table.locator('tbody > tr')).toHaveCount(100);
  const search = page.getByRole('textbox', { name: 'Search meal break employees' });
  const inputPaint = await search.evaluate((element) => {
    const input = element as HTMLInputElement;
    const start = performance.now();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(
      input,
      'Driver 2999',
    );
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return new Promise<{ value: string; ms: number }>((resolve) =>
      requestAnimationFrame(() => resolve({ value: input.value, ms: performance.now() - start })),
    );
  });
  expect(inputPaint.value).toBe('Driver 2999');
  expect(inputPaint.ms).toBeLessThan(100);
  await expect(table.locator('tbody > tr')).toHaveCount(1);
  await expect(table.locator('tbody')).toContainText('Driver 2999');
  await expect(page.locator('.meal-page .paycom-day-results')).toHaveAttribute(
    'aria-busy',
    'false',
  );
  await search.fill('');
  await expect(table.locator('tbody > tr')).toHaveCount(100);
  await table.getByRole('columnheader', { name: 'Employee' }).getByRole('button').click();
  await expect(table.locator('tbody > tr').first()).toContainText('Driver 2999');
  const count = reads;
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export meal breaks', exact: true }).click();
  const csv = fs.readFileSync(await (await download).path(), 'utf8');
  const lines = csv.split('\r\n');
  expect(lines).toHaveLength(3001);
  expect(lines[1]).toContain('Driver 2999');
  expect(lines.at(-1)).toContain('Driver 0000');
  expect(reads).toBe(count);
  test
    .info()
    .annotations.push({ type: '3000-employee input paint ms', description: String(inputPaint.ms) });
});

test('large teams page every member and keep one dismissal listener pair through navigation', async ({
  page,
}) => {
  await page.addInitScript(() => {
    const active = new Map<string, Set<unknown>>();
    const add = EventTarget.prototype.addEventListener;
    const remove = EventTarget.prototype.removeEventListener;
    Object.defineProperty(EventTarget.prototype, 'addEventListener', {
      value: function (
        this: EventTarget,
        type: string,
        listener: EventListenerOrEventListenerObject | null,
        options?: boolean | AddEventListenerOptions,
      ) {
        if (this === document && ['pointerdown', 'keydown'].includes(type)) {
          const listeners = active.get(type) ?? new Set();
          listeners.add(listener);
          active.set(type, listeners);
        }
        return add.call(this, type, listener, options);
      },
    });
    Object.defineProperty(EventTarget.prototype, 'removeEventListener', {
      value: function (
        this: EventTarget,
        type: string,
        listener: EventListenerOrEventListenerObject | null,
        options?: boolean | EventListenerOptions,
      ) {
        if (this === document) active.get(type)?.delete(listener);
        return remove.call(this, type, listener, options);
      },
    });
    Object.assign(window, {
      menuListeners: () => ({
        pointerdown: active.get('pointerdown')?.size ?? 0,
        keydown: active.get('keydown')?.size ?? 0,
      }),
    });
  });
  await page.route('**/api/dsp/members', async (route) => {
    const response = await route.fetch();
    const members: Membership[] = await response.json();
    const sample = members.find((member) => member.name === 'Jordan Ellis')!;
    await route.fulfill({
      response,
      json: Array.from({ length: 250 }, (_, index) => ({
        ...sample,
        id: `member-${index}`,
        name: `Member ${String(index).padStart(3, '0')}`,
        email: `member${index}@dispatch.test`,
      })),
    });
  });
  await page.route('**/api/dsp/invitations', (route) =>
    route.fulfill({
      status: 200,
      json: Array.from({ length: 250 }, (_, index) => ({
        email: `invited${String(index).padStart(3, '0')}@dispatch.test`,
        role: 'Member',
        expiresAt: Date.now() + 86400000,
        accepted: false,
      })),
    }),
  );
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Team & Roles', exact: true }).click();
  const table = page.getByRole('table', { name: 'Team members' });
  await expect(table.locator('tbody > tr')).toHaveCount(100);
  await expect(page.getByLabel(/^Actions for Member /)).toHaveCount(100);
  const counts = () =>
    page.evaluate(() =>
      (
        window as typeof window & { menuListeners: () => { pointerdown: number; keydown: number } }
      ).menuListeners(),
    );
  const initial = await counts();
  expect(initial.pointerdown).toBeLessThan(8);
  expect(initial.keydown).toBeLessThan(8);
  const menu = table.locator('details').first();
  await menu.evaluate((element) => {
    (element as HTMLDetailsElement).open = true;
    element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  });
  await expect(menu).not.toHaveAttribute('open');
  await expect(menu.locator('summary')).toBeFocused();
  await menu.locator('summary').click();
  await expect(menu).toHaveAttribute('open');
  await page.getByRole('heading', { name: 'Team & Roles', exact: true }).click();
  await expect(menu).not.toHaveAttribute('open');
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(table.locator('tbody > tr').first()).toContainText('Member 100');
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(table.locator('tbody > tr')).toHaveCount(50);
  await expect(table.locator('tbody > tr').last()).toContainText('Member 249');
  await page.getByRole('textbox', { name: 'Search members', exact: true }).fill('Member 249');
  await expect(table.locator('tbody > tr')).toHaveCount(1);
  await expect(table.locator('tbody')).toContainText('Member 249');
  await page.getByRole('tab', { name: 'Invitations', exact: true }).click();
  const invitations = page.getByRole('table', { name: 'Pending invitations' });
  await expect(invitations.locator('tbody > tr')).toHaveCount(100);
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(invitations.locator('tbody > tr').first()).toContainText('invited100@dispatch.test');
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(invitations.locator('tbody > tr')).toHaveCount(50);
  await expect(invitations.locator('tbody > tr').last()).toContainText('invited249@dispatch.test');
  await page.getByRole('link', { name: 'Home Page', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'Currently under development', exact: true }),
  ).toBeVisible();
  const after = await counts();
  expect(after.pointerdown).toBeLessThanOrEqual(initial.pointerdown);
  expect(after.keydown).toBeLessThanOrEqual(initial.keydown);
});

test('terminal team reads stop loading and recover members, roles and invitations on retry', async ({
  page,
}) => {
  const allowed = new Set<string>();
  for (const resource of ['members', 'roles', 'invitations'])
    await page.route(`**/api/dsp/${resource}`, (route) =>
      allowed.has(resource)
        ? route.continue()
        : route.fulfill({ status: 400, json: { error: 'invalid_request' } }),
    );
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Team & Roles', exact: true }).click();
  for (const [resource, tab] of [
    ['members', 'Members'],
    ['roles', 'Roles'],
    ['invitations', 'Invitations'],
  ] as const) {
    await page.getByRole('tab', { name: tab, exact: true }).click();
    const retry = page.getByRole('button', { name: 'Retry loading', exact: true });
    await expect(retry).toBeVisible();
    await expect(page.locator('.loading')).toHaveCount(0);
    allowed.add(resource);
    await retry.click();
    await expect(retry).toHaveCount(0);
    if (resource === 'members')
      await expect(page.getByRole('table', { name: 'Team members' })).toBeVisible();
    else if (resource === 'roles')
      await expect(page.getByRole('row', { name: /^Owner/ })).toBeVisible();
    else
      await expect(
        page.getByRole('heading', { name: 'No invitations', exact: true }),
      ).toBeVisible();
  }
});
