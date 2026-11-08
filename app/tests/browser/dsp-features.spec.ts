import type { Locator } from '@playwright/test';
import { test, expect, login } from '../../../core/shell/tests/support/fixtures.js';

// A feature's row in the Features tab's list, which opens its panel.
const item = (pane: Locator, name: string) =>
  pane
    .getByRole('navigation', { name: 'Features', exact: true })
    .getByRole('button')
    .filter({ has: pane.page().getByText(name, { exact: true }) });

test('the platform switches a DSP’s features, their parts and connections, and the team’s pages follow', async ({
  page,
  browser,
}) => {
  await login(page);
  const list = page.getByRole('region', { name: 'DSPs', exact: true });
  await expect(page.getByLabel('DSP summary')).toContainText('Online');
  await list.getByRole('button', { name: /Northline Logistics/ }).click();
  const pane = page.getByRole('region', { name: 'Northline Logistics', exact: true });
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  const toggle = (name: string) => pane.getByRole('switch', { name, exact: true });
  const tab = (name: string) => toggle(`${name} tab`);
  const timecard = toggle('Timecard');
  const cortex = toggle('Cortex');
  // What every DSP has shows on, and nobody switches it.
  for (const name of ['Home Page', 'Team & Roles', 'Settings']) {
    await item(pane, name).click();
    await expect(toggle(name)).toBeChecked();
    await expect(toggle(name)).toBeDisabled();
  }
  // A feature's panel holds its parts.
  await item(pane, 'Timecard').click();
  await expect(timecard).toBeChecked();
  await expect(tab('Meal Breaks')).toBeChecked();

  // A connection takes what needs it along: of Timecard, only its Meal Breaks tab.
  await item(pane, 'Cortex').click();
  await cortex.click();
  const off = page.getByRole('dialog', { name: 'Switch off Cortex?' });
  await expect(off).toContainText('Also switches off:');
  await expect(off.getByRole('listitem')).toHaveText([
    'Routesneeds a route source',
    'DVICneeds a DVIC source',
    'Weekly Scorecardneeds a weekly scorecard source',
    'Driver Profilesneeds an associates source',
    'Daily Performanceneeds a daily performance source',
    'Vansneeds a vans source',
    'Timecard · Meal Breaksneeds a meal-break source',
  ]);
  await off.getByRole('button', { name: 'Switch off', exact: true }).click();
  await expect(cortex).not.toBeChecked();
  for (const name of [
    'Routes',
    'DVIC',
    'Weekly Scorecard',
    'Driver Profiles',
    'Daily Performance',
    'Vans',
  ])
    await expect(item(pane, name)).toContainText('Off');
  await expect(item(pane, 'Uniform Inventory')).toContainText('On');
  await item(pane, 'Timecard').click();
  await expect(timecard).toBeChecked();
  await expect(tab('Meal Breaks')).not.toBeChecked();

  const context = await browser.newContext();
  const member = await context.newPage();
  await login(member, 'member@dispatch.test');
  await expect(member.getByRole('link', { name: 'Uniform Inventory', exact: true })).toBeVisible();
  await member.getByRole('link', { name: 'Timecard', exact: true }).click();
  await expect(member.getByRole('tablist', { name: 'Timecard' }).getByRole('tab')).toHaveText([
    'Timecard',
    'Employees',
  ]);

  // A feature switched off takes its parts along, which wait for it as they were.
  await timecard.click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(timecard).not.toBeChecked();
  await expect(tab('Timecard')).not.toBeChecked();
  await expect(tab('Timecard')).toBeDisabled();
  // The member's next request finds their view expired, and the page gone.
  await member.getByRole('link', { name: 'Uniform Inventory', exact: true }).click();
  await expect(member.getByRole('link', { name: 'Timecard', exact: true })).toHaveCount(0);

  // The role sheet no longer offers the Timecard's permissions, and keeps its other ones.
  await pane.getByRole('button', { name: 'View', exact: true }).click();
  await page.getByRole('link', { name: 'Team & Roles', exact: true }).click();
  await page.getByRole('tab', { name: 'Roles', exact: true }).click();
  await expect(page.getByRole('row', { name: /^Manager/ })).toContainText('View Uniform Inventory');
  await page.getByLabel('Actions for Manager').click();
  await page.getByRole('button', { name: 'Edit role', exact: true }).click();
  const sheet = page.getByRole('dialog', { name: 'Edit Manager' });
  await expect(sheet.getByRole('switch', { name: /^View Uniform Inventory/ })).toBeChecked();
  await expect(sheet.getByRole('switch', { name: /Timecard|Collections/ })).toHaveCount(0);
  await sheet.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('button', { name: 'Exit view', exact: true }).click();
  await context.close();
});

test('a feature switched back on brings its parts as they were, and a part the connection it needs', async ({
  page,
  browser,
  dispatch,
}) => {
  // As the test above leaves the DSP: Cortex off, and the Timecard with it.
  const platform = await dispatch.client();
  const north = platform.session.dsps.find(
    (dsp: { name: string }) => dsp.name === 'Northline Logistics',
  );
  for (const feature of ['cortex', 'timecard']) {
    const off = await platform.post(`/api/platform/dsps/${north.id}/features`, {
      feature,
      enabled: false,
    });
    expect(off.status).toBe(200);
  }
  await login(page);
  const list = page.getByRole('region', { name: 'DSPs', exact: true });
  await list.getByRole('button', { name: /Northline Logistics/ }).click();
  const pane = page.getByRole('region', { name: 'Northline Logistics', exact: true });
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  const toggle = (name: string) => pane.getByRole('switch', { name, exact: true });
  const tab = (name: string) => toggle(`${name} tab`);
  const timecard = toggle('Timecard');
  // A switch that takes nothing with it asks nothing. Switched off and on before the member
  // signs in: a view the member reopened in between would lack the page their next request uses.
  const uniforms = toggle('Uniform Inventory');
  await item(pane, 'Uniform Inventory').click();
  await uniforms.click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(uniforms).not.toBeChecked();
  await uniforms.click();
  await expect(uniforms).toBeChecked();
  const context = await browser.newContext();
  const member = await context.newPage();
  await login(member, 'member@dispatch.test');
  await expect(member.getByRole('link', { name: 'Timecard', exact: true })).toHaveCount(0);

  // Switched back on, the feature brings its parts back as they were.
  await item(pane, 'Timecard').click();
  await timecard.click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(timecard).toBeChecked();
  await expect(tab('Timecard')).toBeChecked();
  await expect(tab('Meal Breaks')).not.toBeChecked();
  // A part brings the connection it needs, and says so first.
  await tab('Meal Breaks').click();
  const on = page.getByRole('dialog', { name: 'Switch on Meal Breaks tab?' });
  await expect(on.getByRole('listitem')).toHaveText([
    'CortexTimecard · Meal Breaks needs a meal-break source',
  ]);
  await on.getByRole('button', { name: 'Switch on', exact: true }).click();
  await expect(tab('Meal Breaks')).toBeChecked();
  await expect(item(pane, 'Cortex')).not.toContainText('Off');
  // A tab switches alone; the last one on takes its page, and says so first.
  await tab('Employee Search').click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(tab('Employee Search')).not.toBeChecked();
  // The next switch goes by what the last one answered, before the DSP's features are read
  // again, so the last tab on still asks first.
  const reading = `**/api/platform/dsps/${north.id}/features`;
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(reading, async (route) => {
    if (route.request().method() === 'GET') await held;
    await route.continue();
  });
  await tab('Timecard').click();
  await tab('Meal Breaks').click();
  const last = page.getByRole('dialog', { name: 'Switch off Meal Breaks tab?' });
  await expect(last.getByRole('listitem')).toHaveText(['Timecardhas no other tab on']);
  await last.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(tab('Meal Breaks')).toBeChecked();
  release();
  await page.unroute(reading);
  await tab('Timecard').click();
  await expect(tab('Timecard')).toBeChecked();
  // The member's next request finds their view expired and reopens it with the page back,
  // less the tab switched off.
  await member.getByRole('link', { name: 'Uniform Inventory', exact: true }).click();
  await member.getByRole('link', { name: 'Timecard', exact: true }).click();
  await expect(member.getByRole('tablist', { name: 'Timecard' }).getByRole('tab')).toHaveText([
    'Timecard',
    'Meal Breaks',
  ]);
  await context.close();
});

test('the platform hides a feature from a DSP, and only the Platform Owner view still shows it', async ({
  page,
  browser,
}) => {
  await login(page);
  const list = page.getByRole('region', { name: 'DSPs', exact: true });
  await list.getByRole('button', { name: /Northline Logistics/ }).click();
  const pane = page.getByRole('region', { name: 'Northline Logistics', exact: true });
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  const uniforms = pane.getByRole('switch', { name: 'Uniform Inventory', exact: true });
  const sight = pane.getByRole('radiogroup', { name: 'Who sees Uniform Inventory' });
  const hide = sight.getByRole('radio', { name: 'Hidden' });
  const show = sight.getByRole('radio', { name: 'Everyone' });
  // What every DSP has is always shown, and a connection has nothing to show.
  for (const name of ['Home Page', 'Team & Roles', 'Settings', 'Cortex', 'Paycom']) {
    await item(pane, name).click();
    await expect(pane.getByRole('region', { name, exact: true })).toBeVisible();
    await expect(pane.getByRole('radiogroup', { name: `Who sees ${name}` })).toHaveCount(0);
  }
  await item(pane, 'Uniform Inventory').click();
  await expect(show).toBeChecked();

  const context = await browser.newContext();
  const member = await context.newPage();
  await login(member, 'member@dispatch.test');
  const memberLink = member.getByRole('link', { name: 'Uniform Inventory', exact: true });
  await expect(memberLink).toBeVisible();

  // Hidden, it stays on, and the member's next request finds it gone.
  await hide.click();
  await expect(hide).toBeChecked();
  await expect(uniforms).toBeChecked();
  await expect(item(pane, 'Uniform Inventory')).toContainText('Hidden');
  await member.getByRole('link', { name: 'Timecard', exact: true }).click();
  await expect(memberLink).toHaveCount(0);

  // The platform owner's own view of the DSP still has it; its owners' view does not.
  await pane.getByRole('button', { name: 'View', exact: true }).click();
  const banner = page.getByRole('region', { name: 'DSP viewing mode' });
  const link = page.getByRole('link', { name: 'Uniform Inventory', exact: true });
  await expect(banner).toContainText('as Platform Owner');
  await expect(banner).toContainText('with the features hidden from the DSP');
  await expect(link).toBeVisible();
  const menu = banner.getByLabel('View as role');
  await menu.click();
  await banner.getByRole('button', { name: 'Owner', exact: true }).click();
  await expect(banner).toContainText('as DSP owner');
  await expect(link).toHaveCount(0);
  await menu.click();
  await banner.getByRole('button', { name: 'Platform Owner', exact: true }).click();
  await expect(banner).toContainText('as Platform Owner');
  await expect(link).toBeVisible();
  await banner.getByRole('button', { name: 'Exit view', exact: true }).click();

  // Shown again, the member's next request has it back.
  await list.getByRole('button', { name: /Northline Logistics/ }).click();
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  await item(pane, 'Uniform Inventory').click();
  await show.click();
  await expect(show).toBeChecked();
  await member.getByRole('link', { name: 'Home Page', exact: true }).click();
  await member.getByRole('link', { name: 'Timecard', exact: true }).click();
  await expect(memberLink).toBeVisible();
  await context.close();
});

test('the platform puts a feature in beta at a DSP, and only the testers it adds see it', async ({
  page,
  browser,
}) => {
  await login(page);
  const list = page.getByRole('region', { name: 'DSPs', exact: true });
  await list.getByRole('button', { name: /Northline Logistics/ }).click();
  const pane = page.getByRole('region', { name: 'Northline Logistics', exact: true });
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  await item(pane, 'Uniform Inventory').click();
  const sight = pane.getByRole('radiogroup', { name: 'Who sees Uniform Inventory' });
  const context = await browser.newContext();
  const member = await context.newPage();
  await login(member, 'member@dispatch.test');
  const memberLink = member.getByRole('link', { name: 'Uniform Inventory', exact: true });
  await expect(memberLink).toBeVisible();
  // Its next request finds it gone, once it is in beta and they don't test it: Timecard's,
  // opened last, as leaving a page drops its requests unanswered.
  const refresh = async () => {
    await member.getByRole('link', { name: 'Home Page', exact: true }).click();
    await member.getByRole('link', { name: 'Timecard', exact: true }).click();
  };

  await sight.getByRole('radio', { name: 'Beta testers' }).click();
  await expect(sight.getByRole('radio', { name: 'Beta testers' })).toBeChecked();
  await refresh();
  await expect(memberLink).toHaveCount(0);

  // Its Beta testers tab lists what is in beta, and adds the member as a tester.
  await pane.getByRole('tab', { name: 'Beta testers', exact: true }).click();
  await expect(pane.getByRole('region', { name: 'In beta' })).toContainText('Uniform Inventory');
  const testers = pane.getByRole('region', { name: 'Testers' });
  await testers.getByRole('button', { name: 'Add tester' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add beta tester' });
  await dialog.getByPlaceholder('Search members').fill('jordan');
  await dialog.getByRole('checkbox').check();
  await dialog.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(testers).toContainText('Jordan Ellis');
  await refresh();
  await expect(memberLink).toBeVisible();

  // Removed, they leave the list at once, and lose it again.
  const removing = '**/beta-testers/*/remove';
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(removing, async (route) => {
    await held;
    await route.continue();
  });
  await testers.getByRole('button', { name: 'Remove Jordan Ellis' }).click();
  await expect(testers).not.toContainText('Jordan Ellis');
  release();
  await page.unroute(removing);
  await expect(page.getByText('Jordan Ellis removed from beta testers')).toBeVisible();
  await expect(testers).not.toContainText('Jordan Ellis');
  await refresh();
  await expect(memberLink).toHaveCount(0);
  await context.close();
});

test('a part reaches no more people than its feature, the list finds a feature by its parts, and the address keeps the one chosen', async ({
  page,
}) => {
  await login(page);
  await page
    .getByRole('region', { name: 'DSPs', exact: true })
    .getByRole('button', { name: /Northline Logistics/ })
    .click();
  const pane = page.getByRole('region', { name: 'Northline Logistics', exact: true });
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  const list = pane.getByRole('navigation', { name: 'Features', exact: true });
  // A part's name finds its feature.
  const search = list.getByLabel('Search features');
  await search.fill('meal');
  await expect(list.getByRole('button').filter({ hasText: 'Timecard' })).toHaveCount(1);
  await expect(item(pane, 'Uniform Inventory')).toHaveCount(0);
  await search.fill('');

  await item(pane, 'Timecard').click();
  const timecard = pane.getByRole('region', { name: 'Timecard', exact: true });
  // The address names it: a reload opens it again, on the Features tab.
  await page.reload();
  await expect(timecard).toBeVisible();
  // Another DSP opened keeps the feature chosen, and Back returns to this one's own.
  await pane.getByRole('button', { name: 'Switch DSP', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Switch DSP' })
    .getByRole('button', { name: /Summit Delivery/ })
    .click();
  const summit = page.getByRole('region', { name: 'Summit Delivery', exact: true });
  await expect(summit.getByRole('region', { name: 'Timecard', exact: true })).toBeVisible();
  await item(summit, 'Uniform Inventory').click();
  await expect(
    summit.getByRole('region', { name: 'Uniform Inventory', exact: true }),
  ).toBeVisible();
  await page.goBack();
  await expect(timecard).toBeVisible();
  // From the DSPs table too, over the tab last open.
  const address = page.url();
  await pane.getByRole('tab', { name: 'Details', exact: true }).click();
  await pane.getByRole('button', { name: 'All DSPs', exact: true }).click();
  await page.goto(address);
  await expect(timecard).toBeVisible();
  const sight = pane.getByRole('radiogroup', { name: 'Who sees Timecard' });
  const meals = pane.getByRole('combobox', { name: 'Who sees Meal Breaks tab' });
  await expect(meals).toHaveValue('shown');
  // In beta, the feature takes its parts there, and what a part was shown to waits for it.
  await sight.getByRole('radio', { name: 'Beta testers' }).click();
  await expect(meals).toHaveValue('beta');
  await expect(meals.getByRole('option', { name: 'Everyone' })).toBeDisabled();
  await sight.getByRole('radio', { name: 'Everyone' }).click();
  await expect(meals).toHaveValue('shown');

  // A part in beta lists its feature among what is in beta.
  await meals.selectOption('beta');
  await expect(meals).toHaveValue('beta');
  await list.getByRole('group', { name: 'Show' }).getByRole('button', { name: /^Beta/ }).click();
  await expect(item(pane, 'Timecard')).toBeVisible();
  await expect(item(pane, 'Home Page')).toHaveCount(0);
  await meals.selectOption('shown');
  await expect(meals).toHaveValue('shown');
});
