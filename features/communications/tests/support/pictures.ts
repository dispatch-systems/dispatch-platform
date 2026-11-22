import type { Locator } from '@playwright/test';

/** A picture of a single pixel, as a PNG. */
export const PIXEL = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==',
  'base64',
);

/** Pastes `picture`, as a PNG named `name`, into `field`, as a screenshot copied is pasted. */
export async function pastePicture(field: Locator, name: string, picture = PIXEL) {
  await field.evaluate(
    (element, { name, bytes }) => {
      const pasted = new DataTransfer();
      pasted.items.add(new File([new Uint8Array(bytes)], name, { type: 'image/png' }));
      element.dispatchEvent(
        new ClipboardEvent('paste', { clipboardData: pasted, bubbles: true, cancelable: true }),
      );
    },
    { name, bytes: [...picture] },
  );
}
