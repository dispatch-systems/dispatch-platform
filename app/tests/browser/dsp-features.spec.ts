import { test, expect, login } from '../../../core/shell/tests/support/fixtures.js';

test('the platform switches a DSP’s pages, tabs and connections, and the team’s pages follow', async ({
  page,
  browser,
}) => {
  await login(page);
  const list = page.getByRole('region', { name: 'DSPs', exact: true });
  await expect(page.getByLabel('DSP summary')).toContainText('Online');
  await list.getByRole('button', { name: /Northline Logistics/ }).click();
  const pane = page.getByRole('region', { name: 'Northline Logistics', exact: true });
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  const areas = pane.getByRole('navigation', { name: 'Feature areas' });
  const area = (name: string) => areas.getByRole('button', { name: new RegExp(`^${name}`) });
  await expect(area('Timecard')).toHaveAttribute('aria-current', 'true');
  await expect(area('Timecard')).toContainText('3/3');
  const cortex = pane.getByRole('switch', { name: 'Cortex', exact: true });
  const timecard = pane.getByRole('switch', { name: 'Timecard page', exact: true });
  await expect(timecard).toBeChecked();
  await area('Connections').click();
  await expect(cortex).toBeChecked();
  await cortex.click();
  const off = page.getByRole('dialog', { name: 'Switch off Cortex?' });
  await expect(off).toContainText('Also switches off:');
  await expect(off.getByRole('listitem')).toHaveText([
    'Timecardneeds a meal-break source',
    'Routesneeds a route source',
    'DVICneeds a DVIC source',
    'Scorecardneeds a scorecard source',
    'Driver Matchneeds a route source',
  ]);
  await off.getByRole('button', { name: 'Switch off', exact: true }).click();
  await expect(cortex).not.toBeChecked();
  for (const name of ['Timecard', 'Routes', 'DVIC', 'Scorecard', 'Driver Match'])
    await expect(area(name)).toContainText('Off');
  await expect(area('Uniform Inventory')).toContainText('On');
  await area('Timecard').click();
  await expect(timecard).not.toBeChecked();
  // A page switched off keeps its tabs' switches, which wait for it.
  await expect(pane.getByRole('switch', { name: 'Meal Breaks tab', exact: true })).toBeChecked();
  await expect(pane.getByRole('switch', { name: 'Meal Breaks tab', exact: true })).toBeDisabled();

  const context = await browser.newContext();
  const member = await context.newPage();
  await login(member, 'member@dispatch.test');
  await expect(member.getByRole('link', { name: 'Uniform Inventory', exact: true })).toBeVisible();
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

  // Switching the page back on brings the connection it needs.
  await list.getByRole('button', { name: /Northline Logistics/ }).click();
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  await timecard.click();
  const on = page.getByRole('dialog', { name: 'Switch on Timecard?' });
  await expect(on.getByRole('listitem')).toHaveText(['CortexTimecard needs a meal-break source']);
  await on.getByRole('button', { name: 'Switch on', exact: true }).click();
  await expect(timecard).toBeChecked();
  await area('Connections').click();
  await expect(cortex).toBeChecked();
  // A switch that takes nothing with it asks nothing.
  await area('Uniform Inventory').click();
  const uniforms = pane.getByRole('switch', { name: 'Uniform Inventory page', exact: true });
  await uniforms.click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(uniforms).not.toBeChecked();
  await uniforms.click();
  await expect(uniforms).toBeChecked();
  // A tab switches alone.
  await area('Timecard').click();
  const tab = (name: string) => pane.getByRole('switch', { name: `${name} tab`, exact: true });
  await tab('Employee Search').click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(tab('Employee Search')).not.toBeChecked();
  await expect(area('Timecard')).toContainText('2/3');
  await expect(pane.getByRole('region', { name: 'Timecard' })).toContainText('2 of 3 tabs');
  // The last tab on takes its page, and says so first.
  await tab('Timecard').click();
  await expect(area('Timecard')).toContainText('1/3');
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
