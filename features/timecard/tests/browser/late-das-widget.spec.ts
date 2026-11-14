import {
  test,
  expect,
  atHome,
  login,
  openDsp,
} from '../../../../core/shell/tests/support/fixtures.js';

test('Late DAs on Home follows the DSP’s late rule as it changes, and opens Meal Breaks on today’s late DAs', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const dsp = owner.session.dsps.find(
    (item: { name: string }) => item.name === 'Northline Logistics',
  );
  await owner.select(dsp.id);
  await login(page);
  await openDsp(page, dsp.name);
  await atHome(page);
  await page.getByRole('button', { name: 'Add widget', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Late DAs', exact: true }).click();
  await page.getByRole('button', { name: 'Done', exact: true }).click();
  const widget = page.getByRole('article', { name: 'Late DAs' });
  // Today's demo DAs all punched in at 8:00, before the 10:01 they'd be late at.
  await expect(widget).toContainText('0 late');
  await expect(widget).toContainText('At or after 10:01 AM');
  // The DSP moves its rule to 8:00: each of them is late now, here as on Meal Breaks.
  const { sections } = (await owner.get('/api/dsp/settings')).value;
  const rule = sections.find((section: { id: string }) => section.id === 'timecard.late_das');
  const saved = await owner.post('/api/dsp/settings/timecard.late_das', {
    values: { time: '08:00', departments: [] },
    revision: rule.revision,
  });
  expect(saved.status).toBe(200);
  await page.reload();
  const foot = widget.getByText(/^\d+ late · at or after 8:00 AM$/);
  await expect(foot).toBeVisible();
  const late = (await foot.textContent())!.split(' ')[0];
  await expect(widget.getByRole('listitem').first()).toContainText('8:00 AM');
  await widget.getByRole('link').click();
  await expect(page.getByRole('tab', { name: 'Meal Breaks', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(page.getByRole('button', { name: `Late DAs ${late}`, exact: true })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.locator('.meal-table tbody > tr')).toHaveCount(Number(late));
});
