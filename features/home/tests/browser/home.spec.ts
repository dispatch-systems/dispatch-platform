import {
  test,
  expect,
  atHome,
  login,
  openDsp,
} from '../../../../core/shell/tests/support/fixtures.js';

test('a member opens their DSP on its home page, blank until they add a widget, and still being built in a window as narrow as a drawer', async ({
  page,
}) => {
  await login(page, 'member@dispatch.test');
  await expect(page.getByRole('heading', { name: 'Home', exact: true, level: 1 })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Add widget', exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Home Page', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await page.setViewportSize({ width: 980, height: 900 });
  await expect(
    page.getByRole('heading', { name: 'Currently under development', exact: true }),
  ).toBeVisible();
  await expect(page.getByText('We’re building your DSP home page.', { exact: true })).toBeVisible();
});

test('a resize the pointer is taken from, as a menu takes it, is kept as the page shows it', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const dsp = owner.session.dsps.find(
    (item: { name: string }) => item.name === 'Northline Logistics',
  );
  await owner.select(dsp.id);
  // Sizes are read as they change, not while they glide.
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await login(page);
  await openDsp(page, dsp.name);
  await atHome(page);
  await page.getByRole('button', { name: 'Add widget', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Late DAs', exact: true }).click();
  const widget = page.getByRole('article', { name: 'Late DAs' });
  const width = async () => (await widget.boundingBox())!.width;
  const added = (await widget.boundingBox())!;
  // Its right edge dragged its own width further, then a second button pressed: the menu that
  // opens takes the pointer as it next moves, and the release never reaches the edge.
  const [edge, middle] = [added.x + added.width, added.y + added.height / 2];
  await page.mouse.move(edge, middle);
  await page.mouse.down();
  await page.mouse.move(edge + added.width, middle, { steps: 4 });
  await page.mouse.down({ button: 'right' });
  await page.mouse.up({ button: 'right' });
  await page.mouse.move(edge + added.width + 30, middle, { steps: 3 });
  await page.mouse.up();
  await expect.poll(width).toBeGreaterThan(added.width * 1.5);
  const resized = await width();
  // What the page keeps is what it showed: the same size once the page is no longer arranged.
  await page.getByRole('button', { name: 'Done', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Edit', exact: true })).toBeVisible();
  await expect.poll(async () => Math.abs((await width()) - resized)).toBeLessThan(2);
});
