import { BESIDE } from '../page/useLayout.js';

// Where Message display's preview shows the page, and how big, in the room beside the choices.

type Size = { width: number; height: number };

/** The app's sidebar beside the page, and its heading above it, on a desktop. */
const SIDEBAR = 240;
const HEADING = 150;
const PHONE = { width: 390, height: 760 };
/** A phone's edge, round its screen. */
const PHONE_EDGE = 20;
/** The preview's page's padding at each side on a desktop's screen (display.css). */
const GUTTER = 36;
/** The least room a desktop's whole page is shown in at its actual size: list and conversation. */
const WHOLE_LEAST = 900;
/** The least room a desktop's page is shown in at its actual size at all. */
export const ACTUAL_LEAST = 420;

/** The screen previewed, `aside` of it kept out of view at its left, made `scale` times its size. */
export type Frame = Size & { scale: number; aside: number };

/**
 * The screen previewed in `room`: a phone at its actual size, in as much of one as there's room
 * for inside its edge; a desktop at its actual size, the whole page while there's room for its
 * list and conversation, else the conversation alone, the page laid out wider by its list (as
 * wide as `list`), which is kept out of view, while that's wide enough for both, else the page
 * as wide as the room, one pane; or, to `fit`, or with no room for a page at its actual size,
 * the whole page as wide as the member's window, made smaller to fit.
 */
export function frame({
  room,
  phone,
  fit,
  list,
  window,
}: {
  room: Size;
  phone: boolean;
  fit: boolean;
  list: number;
  window: Size;
}): Frame {
  if (phone)
    return {
      width: Math.min(PHONE.width, room.width - PHONE_EDGE),
      height: Math.min(PHONE.height, room.height - PHONE_EDGE),
      scale: 1,
      aside: 0,
    };
  if (!fit && room.width >= ACTUAL_LEAST) {
    // The list as the page draws it, no more than 45% of the page, which it widens.
    const drawn = Math.min(list, (room.width * 9) / 11);
    const alone = room.width < WHOLE_LEAST && room.width + drawn - 2 * GUTTER >= BESIDE;
    const aside = alone ? drawn : 0;
    return { width: room.width + aside, height: room.height, scale: 1, aside };
  }
  const whole = { width: window.width - SIDEBAR, height: window.height - HEADING };
  const scale = Math.min(1, room.width / whole.width, room.height / whole.height);
  return { ...whole, scale, aside: 0 };
}
