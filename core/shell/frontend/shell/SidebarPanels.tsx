import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import type { Route } from '../runtime/slots.js';

type Sidebar = NonNullable<Route['sidebar']>;
/** What fills the sidebar above the account: a page's own sidebar, or the app's name and pages. */
type Panel = Sidebar | undefined;
/** A panel leaving, drawn as it was when it left: a page's sidebar never draws for another page. */
type Leaving = { panel: Panel; drawn: ReactNode };

/** How long a panel takes to slide, quick to start and soft to stop. */
const slide = { duration: 260, easing: 'cubic-bezier(0.2, 0, 0, 1)' };

// Each panel's key and place in the page, the app's first, kept while one slides for another: a
// panel moved in the page would lose its scroll.
let placed = 0;
const places = new WeakMap<Sidebar, number>();
function place(panel: Panel) {
  if (!panel) return 0;
  if (!places.has(panel)) places.set(panel, ++placed);
  return places.get(panel)!;
}

/**
 * The sidebar's panel `shown`, as `draw` draws it. When another is shown, the panels slide
 * together, the one shown pushing the others out: in from the right for a page's own sidebar,
 * and from the left back to the app's, as the app reads left to right. Those leaving are out of
 * reach, and gone once they're out. Only the panels' `translate` moves, which the browser runs
 * off the main thread, so a page opening beside them can't stall it; nothing reads the layout.
 */
export function SidebarPanels({
  shown,
  draw,
}: {
  shown: Panel;
  draw: (panel: Panel) => ReactNode;
}) {
  const [panels, setPanels] = useState<{
    shown: Panel;
    slide?: {
      leaving: Leaving[];
      /** Whether a slide was under way, so the panels on screen start from where it left them. */
      midway: boolean;
      /** Whether the panel shown is one that was leaving, turned back. */
      returns: boolean;
    };
  }>({ shown });
  // What the panel shown drew when last on screen, for when it leaves.
  const last = useRef<ReactNode>(null);
  if (shown !== panels.shown) {
    const leaving = panels.slide?.leaving ?? [];
    setPanels({
      shown,
      slide: {
        leaving: [
          { panel: panels.shown, drawn: last.current },
          ...leaving.filter(({ panel }) => panel !== shown),
        ],
        midway: Boolean(panels.slide),
        returns: leaving.some(({ panel }) => panel === shown),
      },
    });
  }
  const drawing = draw(panels.shown);
  useLayoutEffect(() => {
    last.current = drawing;
  });
  const elements = useRef(new Map<number, HTMLElement>());
  useLayoutEffect(() => {
    if (!panels.slide) return;
    const { leaving, midway, returns } = panels.slide;
    // Where a panel on screen is, in sidebar widths: the one shown before at rest, unless a
    // slide was under way, which stops where it is.
    const at = (panel: HTMLElement) => {
      if (!midway) return 0;
      const now = parseFloat(getComputedStyle(panel).translate) || 0;
      for (const moving of panel.getAnimations()) moving.cancel();
      return now;
    };
    const out = leaving.map(({ panel }) => {
      const element = elements.current.get(place(panel))!;
      return { element, from: at(element) };
    });
    const into = elements.current.get(place(panels.shown))!;
    const leavingAt = out.map(({ from }) => from);
    // The panel shown turns back from where it is, or comes in beside the outermost on its side.
    const start = returns
      ? at(into)
      : panels.shown
        ? Math.max(...leavingAt) + 100
        : Math.min(...leavingAt) - 100;
    // With reduced motion, the panel shown takes the others' place at once.
    const timing = matchMedia('(prefers-reduced-motion: reduce)').matches
      ? { ...slide, duration: 0 }
      : slide;
    const moves = [
      into.animate({ translate: [`${start}%`, '0'] }, timing),
      // Held out once there, until they're gone.
      ...out.map(({ element, from }) =>
        element.animate(
          { translate: [`${from}%`, `${from - start}%`] },
          { ...timing, fill: 'forwards' },
        ),
      ),
    ];
    let live = true;
    Promise.all(moves.map((move) => move.finished)).then(
      () => live && setPanels({ shown: panels.shown }),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [panels]);
  return (
    <div className="sidebar-panels">
      {[{ panel: panels.shown, drawn: drawing }, ...(panels.slide?.leaving ?? [])]
        .sort((one, other) => place(one.panel) - place(other.panel))
        .map(({ panel, drawn }) => {
          const leaving = panel !== panels.shown;
          return (
            <div
              key={place(panel)}
              ref={(element) => {
                elements.current.set(place(panel), element!);
                return () => void elements.current.delete(place(panel));
              }}
              className="sidebar-panel"
              // Hidden as well as inert: tools finding controls by role, as the browser tests
              // do, pass over only what's hidden.
              inert={leaving}
              aria-hidden={leaving || undefined}
            >
              {drawn}
            </div>
          );
        })}
    </div>
  );
}
