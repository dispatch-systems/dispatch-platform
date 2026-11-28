import { test, expect, login } from '../../../shell/tests/support/fixtures.js';
import { asked, fromSite } from '../support/demo-requests.js';

test('New Clients opens a request beside its list, keeps a status it is given, shows the same requests as a board, and deletes one', async ({
  page,
  dispatch: f,
}) => {
  await fromSite(f, asked('a'));
  // An address may hold what a link reads as its own: here, a subject.
  const email = 'luis?subject=x@ironwood.example';
  await fromSite(
    f,
    asked('b', { name: 'Luis Carrasco', email, dsp: 'Ironwood Parcel', message: '' }),
  );
  await login(page);
  const sidebar = page.getByRole('link', { name: /New Clients/ });
  await expect(sidebar.getByLabel('2 new')).toBeVisible();
  await sidebar.click();

  // The newest is open beside the list until another is chosen, and its reply goes to its
  // address whole.
  const newest = page.getByRole('region', { name: 'Luis Carrasco' });
  const reply = new URL(
    (await newest.getByRole('link', { name: 'Reply by email' }).getAttribute('href'))!,
  );
  expect([decodeURIComponent(reply.pathname), reply.search]).toEqual([email, '']);
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

  // The board holds each under its status, with no request open over it, and stays the view
  // after a reload.
  await page.getByRole('button', { name: 'Board' }).click();
  const booked = page.getByRole('region', { name: 'Demo booked' });
  await expect(booked.getByRole('button', { name: /Maria Alvarez/ })).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.reload();
  await expect(
    page
      .getByRole('region', { name: 'New', exact: true })
      .getByRole('button', { name: /Luis Carrasco/ }),
  ).toBeVisible();
  await booked.getByRole('button', { name: /Maria Alvarez/ }).click();
  const sheet = page.getByRole('dialog', { name: 'Maria Alvarez' });
  await expect(sheet).toBeVisible();

  // Deleting asks first, then the request is gone from the board and nothing stays open.
  await sheet.getByRole('button', { name: 'Delete' }).click();
  await page
    .getByRole('dialog', { name: 'Delete this request?' })
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(page.getByText('Request deleted', { exact: true })).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Maria Alvarez/ })).toHaveCount(0);
});
