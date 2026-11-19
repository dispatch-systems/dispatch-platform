// The page's scroll, stopped while anything holds it: dialogs open over one another and close in
// any order, and the page scrolls again once the last lets go.

let held = 0;
let before = '';

/** Stops the page scrolling until the returned release, and every other lock's, is called. */
export function lockScroll() {
  if (held++ === 0) {
    before = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
  }
  return () => {
    if (--held === 0) document.body.style.overflow = before;
  };
}
