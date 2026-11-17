import {
  test,
  expect,
  login,
  openDsp,
} from '../../../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, and its owner holds every permission.

test("Conduct's tab strip links to the address the DSP set, in a new browser tab", async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.post(`/api/platform/dsps/${dsp.id}/custom-id`, { customId: 'cjf87gqd' });
  await owner.post(`/api/platform/dsps/${dsp.id}/features`, {
    feature: 'cjf87gqd_legacy_tracker',
    enabled: true,
  });
  await owner.select(dsp.id);
  const link = 'https://tracker.example.com/sheet';
  const saved = await owner.post('/api/dsp/settings/cjf87gqd_legacy_tracker.general', {
    values: { link },
    revision: 0,
  });
  expect(saved.status).toBe(200);
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Conduct', exact: true }).click();
  const tracker = page.getByRole('link', { name: 'Legacy Tracker', exact: true });
  await expect(tracker).toHaveAttribute('href', link);
  await expect(tracker).toHaveAttribute('target', '_blank');
  // Conduct's own tabs still switch beside it.
  await page.getByRole('tab', { name: 'Drivers', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Drivers', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(tracker).toBeVisible();
});
