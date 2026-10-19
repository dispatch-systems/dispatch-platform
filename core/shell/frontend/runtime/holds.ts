// A page with unsaved edits holds the address: however it moves, by a link followed, Back, the
// browser's Back or Forward or a hash typed, the page asks first, and while it asks the move is
// undone, so the address names the page still open and nothing following it hears the move. Once
// every hold lets it, it is made again as it would have been, and every page following the
// address hears it then; a hold that never lets it keeps the page, at its address. A page drawn
// anew where the address stays, as for another role viewed as, or left with the address staying,
// as by signing out, asks its holds first too (`askToLeave`). A page asks the editors open on it
// through its hold (`leave.ts`).
//
// Where the browser numbers the tab's history (its Navigation API), a move along it is undone
// by going back to the page's entry, and made again by going on to the one it moved to, so the
// history stays as it was: kept, the page's Back and Forward still lead where they led. A link
// followed or a hash typed has added its entry before the page is asked, in place of any ahead
// of the page's: kept, the page has it ahead, and Forward leads there, asking again. A move that
// replaced its entry is put back in place, as it was. In a browser that doesn't number them,
// every move is put back in place, in the entry it moved to: kept, the page is then at that entry
// too, and Back or Forward to it stays on the page.

/** Asked before the page open is left: runs `then` at once, once let, or never. */
export type Hold = (then: () => void) => void;
/** Asks each of `asks` in turn, each once the one before has let it, and then runs `then`. */
export function inTurn(asks: readonly Hold[], then: () => void) {
  const next = (index: number): void =>
    index < asks.length ? asks[index]!(() => next(index + 1)) : then();
  next(0);
}

const holds = new Set<Hold>();
/** Has the page ask `hold` before it is left, until the answer is called. */
export function holdAddress(hold: Hold) {
  followNumbers();
  holds.add(hold);
  return () => void holds.delete(hold);
}
/**
 * Asks the holds before the page open is drawn anew where the address stays, or left with the
 * address staying, as viewing its DSP as another role draws it and signing out leaves it, and runs
 * `then` once every one has let it.
 */
export const askToLeave = (then: () => void) => inTurn([...holds], then);

/** The tab's history, where the browser numbers its entries: the one open, and each move. */
type Numbered = EventTarget & { currentEntry: { index: number } | null };
const numbered = () => (globalThis as { navigation?: Numbered }).navigation;
/** The numbered history followed, and its latest move from one entry to another. */
let followed: Numbered | undefined;
let latest: { from: number; to: number } | undefined;
function followNumbers() {
  const tab = numbered();
  if (!tab || tab === followed) return;
  followed = tab;
  tab.addEventListener('currententrychange', (event) => {
    // An entry replaced is no longer numbered.
    const from = (event as Event & { from: { index: number } }).from.index;
    const to = tab.currentEntry?.index;
    latest = from >= 0 && to !== undefined ? { from, to } : undefined;
  });
}
/** A move undone along the history, by the address it returns to, which nothing else hears. */
let undone: string | undefined;
/** The move the holds let, by the address it is made to, as it is made again. */
let released: string | undefined;
/**
 * Asks the holds as the address moves, heard before anything else that follows it: the app
 * listens with it first, before it draws a page.
 */
export function askHolds(event: HashChangeEvent) {
  const { oldURL: from, newURL: to } = event;
  if (to === undone) {
    undone = undefined;
    return event.stopImmediatePropagation();
  }
  if (to === released) return void (released = undefined);
  if (!holds.size) return;
  // How far along the history it moved, if it did.
  const at = numbered()?.currentEntry?.index;
  const moved = at !== undefined && latest?.to === at ? at - latest.from : 0;
  // Let at once, the move goes on as it began; asked first, it is made again once let.
  let waiting = false;
  let letGo = false;
  const go = () => {
    if (!waiting) return void (letGo = true);
    released = to;
    if (moved) history.go(moved);
    else {
      history.replaceState(history.state, '', to);
      window.dispatchEvent(new HashChangeEvent('hashchange', { oldURL: from, newURL: to }));
    }
  };
  inTurn([...holds], go);
  if (letGo) return;
  waiting = true;
  event.stopImmediatePropagation();
  if (moved) {
    undone = from;
    history.go(-moved);
  } else history.replaceState(history.state, '', from);
}
