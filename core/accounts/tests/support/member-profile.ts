import { expect, type Page } from '@playwright/test';

/** The member profile page fits the window: its heading and actions show, and nothing scrolls. */
export async function fits(page: Page) {
  await expect
    .poll(() =>
      page.evaluate(() => {
        const heading = document.querySelector('h1')!.getBoundingClientRect();
        const back = document.querySelector('.member-profile-actions')!.getBoundingClientRect();
        return (
          document.documentElement.scrollWidth <= innerWidth &&
          document.documentElement.scrollHeight <= innerHeight &&
          heading.top >= 0 &&
          back.bottom <= innerHeight &&
          back.left >= 0 &&
          back.right <= innerWidth
        );
      }),
    )
    .toBe(true);
}
