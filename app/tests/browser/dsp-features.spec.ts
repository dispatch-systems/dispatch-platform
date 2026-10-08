import { test, expect, login } from '../../../core/shell/tests/support/fixtures.js';

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
  const features = pane.getByRole('region', { name: 'Features', exact: true });
  const toggle = (name: string) => pane.getByRole('switch', { name, exact: true });
  const tab = (name: string) => toggle(`${name} tab`);
  const timecard = toggle('Timecard');
  // A feature's row: the list item holding its own switch.
  const row = (name: string) =>
    features
      .getByRole('listitem')
      .filter({ has: page.getByRole('switch', { name, exact: true }) })
      .first();
  const cortex = toggle('Cortex');
  // What every DSP has shows on, and nobody switches it.
  for (const name of ['Home Page', 'Team & Roles', 'Settings']) {
    await expect(toggle(name)).toBeChecked();
    await expect(toggle(name)).toBeDisabled();
  }
  await expect(timecard).toBeChecked();
  await expect(row('Timecard')).toContainText('3 of 3 parts');
  // A feature's parts show once it is opened.
  await expect(tab('Meal Breaks')).toHaveCount(0);
  await features.getByRole('button', { name: "Timecard's parts", exact: true }).click();
  await expect(tab('Meal Breaks')).toBeChecked();

  // A connection takes what needs it along: of Timecard, only its Meal Breaks tab.
  await cortex.click();
  const off = page.getByRole('dialog', { name: 'Switch off Cortex?' });
  await expect(off).toContainText('Also switches off:');
  await expect(off.getByRole('listitem')).toHaveText([
    'Routesneeds a route source',
    'DVICneeds a DVIC source',
    'Weekly Scorecardneeds a weekly scorecard source',
    'Driver Matchneeds a route source',
    'Daily Performanceneeds a daily performance source',
    'Timecard · Meal Breaksneeds a meal-break source',
  ]);
  await off.getByRole('button', { name: 'Switch off', exact: true }).click();
  await expect(cortex).not.toBeChecked();
  for (const name of ['Routes', 'DVIC', 'Weekly Scorecard', 'Driver Match', 'Daily Performance'])
    await expect(toggle(name)).not.toBeChecked();
  await expect(timecard).toBeChecked();
  await expect(tab('Meal Breaks')).not.toBeChecked();
  await expect(toggle('Uniform Inventory')).toBeChecked();

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

  // Switched back on, the feature brings its parts back as they were.
  await list.getByRole('button', { name: /Northline Logistics/ }).click();
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  await timecard.click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(timecard).toBeChecked();
  await features.getByRole('button', { name: "Timecard's parts", exact: true }).click();
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
  await expect(cortex).toBeChecked();
  // A switch that takes nothing with it asks nothing.
  const uniforms = toggle('Uniform Inventory');
  await uniforms.click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(uniforms).not.toBeChecked();
  await uniforms.click();
  await expect(uniforms).toBeChecked();
  // A tab switches alone; the last one on takes its page, and says so first.
  await tab('Employee Search').click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(tab('Employee Search')).not.toBeChecked();
  await expect(row('Timecard')).toContainText('2 of 3 parts');
  await tab('Timecard').click();
  await tab('Meal Breaks').click();
  const last = page.getByRole('dialog', { name: 'Switch off Meal Breaks tab?' });
  await expect(last.getByRole('listitem')).toHaveText(['Timecardhas no other tab on']);
  await last.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(tab('Meal Breaks')).toBeChecked();
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
  const hide = pane.getByRole('button', { name: 'Hide Uniform Inventory from the DSP' });
  const show = pane.getByRole('button', { name: 'Show Uniform Inventory to the DSP' });
  // What every DSP has is always shown, and a connection has nothing to show.
  for (const name of ['Home Page', 'Team & Roles', 'Settings', 'Cortex', 'Paycom'])
    await expect(pane.getByRole('button', { name: `Hide ${name} from the DSP` })).toHaveCount(0);
  await expect(hide).toHaveAttribute('aria-pressed', 'false');

  const context = await browser.newContext();
  const member = await context.newPage();
  await login(member, 'member@dispatch.test');
  const memberLink = member.getByRole('link', { name: 'Uniform Inventory', exact: true });
  await expect(memberLink).toBeVisible();

  // Hidden, it stays on, and the member's next request finds it gone.
  await hide.click();
  await expect(show).toHaveAttribute('aria-pressed', 'true');
  await expect(uniforms).toBeChecked();
  // Its row: the list item holding its own switch.
  const row = pane
    .getByRole('listitem')
    .filter({ has: page.getByRole('switch', { name: 'Uniform Inventory', exact: true }) })
    .first();
  await expect(row).toContainText('Hidden from DSP');
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
  await show.click();
  await expect(hide).toHaveAttribute('aria-pressed', 'false');
  await member.getByRole('link', { name: 'Home Page', exact: true }).click();
  await member.getByRole('link', { name: 'Timecard', exact: true }).click();
  await expect(memberLink).toBeVisible();
  await context.close();
});
