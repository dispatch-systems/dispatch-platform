import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// GIFs of KLIPY's, from the server's stand-in for KLIPY in fixture mode: a page of them in many
// shapes, each kept to its own, which the panel scrolls through; the one chosen goes as a text.

test('Choose GIF shows each GIF at its own shape, scrolls through them, and sends the one chosen', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Communications' }).click();
  await page
    .getByRole('region', { name: 'Conversations' })
    .getByRole('listitem')
    .filter({ hasText: '(425) 555-0199' })
    .getByRole('button')
    .first()
    .click();
  const conversation = page.getByRole('region', { name: '(425) 555-0199' });
  await expect(conversation.getByLabel('Message')).toBeEnabled();
  await conversation.getByRole('button', { name: 'Attach', exact: true }).click();
  await conversation.getByRole('button', { name: 'Choose GIF' }).click();
  const panel = page.getByRole('dialog', { name: 'Choose GIF' });
  await expect(panel.getByPlaceholder('Search KLIPY')).toBeFocused();

  // Thumbs up is 220 by 124: as wide as its column, and as tall as that shape makes it.
  const thumbs = panel.getByRole('button', { name: 'Thumbs up' });
  const box = (await thumbs.boundingBox())!;
  expect(Math.abs(box.height - (box.width * 124) / 220)).toBeLessThan(2);
  // On my way is taller than wide; the page of them runs past the panel, which scrolls.
  const tall = (await panel.getByRole('button', { name: 'On my way' }).boundingBox())!;
  expect(tall.height).toBeGreaterThan(tall.width);
  const grid = panel.locator('.gif-grid');
  expect(await grid.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);

  // Enter searches, never sending; the one chosen goes with what's written.
  const search = panel.getByPlaceholder('Search KLIPY');
  await search.fill('thumbs');
  await search.press('Enter');
  await expect(panel.getByRole('button')).toHaveCount(1);
  await panel.getByRole('button', { name: 'Thumbs up' }).click();
  await expect(panel).toHaveCount(0);
  await conversation.getByLabel('Message').fill('Nice work');
  await conversation.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(conversation.getByRole('img', { name: 'Thumbs up' })).toBeVisible();
  await expect(conversation.getByText('Nice work', { exact: true })).toBeVisible();
});
