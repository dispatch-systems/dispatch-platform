import { test, expect, login } from '../../../shell/tests/support/fixtures.js';
import { asked, fromSite } from '../support/demo-requests.js';

test('New Clients opens a request beside its list, keeps a status it is given, and shows the same requests as a board', async ({
  page,
  dispatch: f,
}) => {
  await fromSite(f, asked('a'));
  await fromSite(f, asked('b', { name: 'Luis Carrasco', dsp: 'Ironwood Parcel', message: '' }));
  await login(page);
  const sidebar = page.getByRole('link', { name: /New Clients/ });
  await expect(sidebar.getByLabel('2 new')).toBeVisible();
  await sidebar.click();

  // The newest is open beside the list until another is chosen.
  await expect(page.getByRole('region', { name: 'Luis Carrasco' })).toBeVisible();
  await page.getByRole('button', { name: /Maria Alvarez/ }).click();
  const open = page.getByRole('region', { name: 'Maria Alvarez' });
  await expect(open.getByText(asked('a').message)).toBeVisible();

  // A status shows at once, in the request, its row, the counts and the sidebar, and is kept.
  await open.getByLabel('Status').selectOption('Demo booked');
  const counts = page.getByRole('group', { name: 'Requests by status' });
  await expect(counts.getByRole('button', { name: '1 Demo booked' })).toBeVisible();
  await expect(sidebar.getByLabel('1 new')).toBeVisible();
  await page.reload();
  await expect(
    page.getByRole('region', { name: 'Maria Alvarez' }).getByLabel('Status'),
  ).toHaveValue('demo');

  // The board holds each under its status, and stays the view after a reload.
  await page.getByRole('button', { name: 'Board' }).click();
  const booked = page.getByRole('region', { name: 'Demo booked' });
  await expect(booked.getByRole('button', { name: /Maria Alvarez/ })).toBeVisible();
  await expect(
    page
      .getByRole('region', { name: 'New', exact: true })
      .getByRole('button', { name: /Luis Carrasco/ }),
  ).toBeVisible();
  await booked.getByRole('button', { name: /Maria Alvarez/ }).click();
  await expect(page.getByRole('dialog', { name: 'Maria Alvarez' })).toBeVisible();
  await page.reload();
  await expect(page.getByRole('region', { name: 'Demo booked' })).toBeVisible();
});
