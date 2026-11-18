import { expect, type Page } from '@playwright/test';

/** The windows a widget fits each size in: a desktop's, and a laptop's. */
const WINDOWS = [
  { width: 1920, height: 1080 },
  { width: 1440, height: 900 },
] as const;

/**
 * What's wrong with each widget on the grid the page shows: one that failed to draw, one still
 * showing nothing below its header, another widget than the page's heading names, a header whose
 * label is cut short, or anything of it its frame cuts off. What a widget clips on purpose, such
 * as the rows a list has no room for or a name ending in an ellipsis, is clipped by its own part,
 * inside the frame, and isn't counted.
 */
function problems(page: Page) {
  return page.locator('.home-cell').evaluateAll((cells) => {
    const shown = document.querySelector('.page-heading h1')?.textContent;
    return cells.flatMap((cell) => {
      const size = `${cell.getAttribute('data-columns')}×${cell.getAttribute('data-rows')}`;
      const frame = cell.querySelector('.home-widget')!;
      if (frame.querySelector('.home-widget-failed')) return [`${size}: it couldn’t load`];
      const widget = frame.querySelector('.widget');
      if (!widget?.querySelector(':scope > :not(header)'))
        return [`${size}: nothing below its header`];
      const found: string[] = [];
      const label = widget.querySelector(':scope > header strong')!;
      // One column may give it a shorter label.
      if (cell.getAttribute('data-columns') !== '1' && label.textContent !== shown)
        found.push(`${size}: it shows ${label.textContent}`);
      const [line, header] = [
        label.getBoundingClientRect(),
        label.parentElement!.getBoundingClientRect(),
      ];
      if (
        label.scrollWidth > label.clientWidth ||
        line.top < header.top ||
        line.bottom > header.bottom + 1
      )
        found.push(`${size}: its header's label "${label.textContent}" is cut short`);
      const edge = frame.getBoundingClientRect();
      for (const part of widget.querySelectorAll('*')) {
        const box = part.getBoundingClientRect();
        if (!box.width || !box.height || getComputedStyle(part).visibility === 'hidden') continue;
        // The nearest part that clips it: a part of the widget's own, or else the frame.
        let clip = part.parentElement!;
        while (clip !== frame && getComputedStyle(clip).overflow === 'visible')
          clip = clip.parentElement!;
        if (clip !== frame && clip !== widget) continue;
        const past = Math.max(
          edge.left - box.left,
          box.right - edge.right,
          edge.top - box.top,
          box.bottom - edge.bottom,
        );
        if (past > 1) {
          const name = part.textContent?.trim().slice(0, 40) || part.tagName.toLowerCase();
          found.push(`${size}: "${name}" runs ${Math.round(past)}px past its frame`);
        }
      }
      return found;
    });
  });
}

/**
 * Opens Home at every size the widget `id` declares, as `?sizes=` shows it away from Production,
 * in each of `WINDOWS`, and fails naming each size it doesn't fit, on every grid of them: in the
 * DSP `dspId`, as the page's signed-in account sees it, with whatever its data is then.
 */
export async function expectEverySize(page: Page, dspId: string, id: string) {
  const window = page.viewportSize();
  for (const size of WINDOWS) {
    await page.setViewportSize(size);
    await page.goto(`/#dsp/${dspId}/overview?sizes=${id}`);
    // Home shows the member's own widgets for one the DSP doesn't have.
    const shown = page.locator('.home-cell').first();
    await expect(shown, `${id} in the DSP`).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Home', exact: true, level: 1 })).toHaveCount(0);
    const next = page.locator('.page-heading').getByRole('button', { name: 'Next', exact: true });
    for (let grid = 1; ; grid++) {
      await expect
        .poll(() => problems(page), {
          message: `${id} in ${size.width}×${size.height}, grid ${grid}`,
          timeout: 10_000,
        })
        .toEqual([]);
      if (!(await next.count()) || !(await next.isEnabled())) break;
      await next.click();
      await expect(page).toHaveURL(new RegExp(`page=${grid + 1}`));
    }
  }
  if (window) await page.setViewportSize(window);
}
