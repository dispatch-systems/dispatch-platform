import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import type { Route } from '../runtime/slots.js';

type Sidebar = NonNullable<Route['sidebar']>;
/** What fills the sidebar above the account: a page's own sidebar, or the app's name and pages. */
type Panel = Sidebar | undefined;

/** How long a panel takes to slide, quick to start and soft to stop. */
const slide = { duration: 260, easing: 'cubic-bezier(0.2, 0, 0, 1)' };

// Each panel keeps its place in the page while one slides for another, the app's first: a panel
// moved in the page would lose its scroll.
let placed = 0;
const places = new WeakMap<Sidebar, number>();
function place(panel: Panel) {
  if (!panel) return 0;
  if (!places.has(panel)) places.set(panel, ++placed);
  return places.get(panel)!;
}

/**
 * The sidebar's panel `shown`, as `draw` draws it. When another is shown, the one leaving slides
 * out as it slides in: to the left toward a page's own sidebar, to the right back to the app's.
 * The leaving panel is out of reach, and gone once it's out. Only the panels' `translate`
 * moves, which the browser runs off the main thread, so a page opening beside them can't stall
 * it; nothing reads the layout.
 */
export function SidebarPanels({
  shown,
  draw,
}: {
  shown: Panel;
  draw: (panel: Panel) => ReactNode;
}) {
  const [panels, setPanels] = useState({ shown, leaving: [] as Panel[], midway: false });
  if (shown !== panels.shown)
    // A slide begun before the last one ended starts each panel from where it is.
    setPanels({ shown, leaving: [panels.shown], midway: panels.leaving.length > 0 });
  const elements = useRef(new Map<Panel, HTMLElement>());
  useLayoutEffect(() => {
    if (!panels.leaving.length) return;
    const into = elements.current.get(panels.shown)!;
    const out = elements.current.get(panels.leaving[0])!;
    const [arrives, leaves] = panels.shown ? ['100%', '-100%'] : ['-100%', '100%'];
    const from = (panel: HTMLElement, otherwise: string) => {
      if (!panels.midway) return otherwise;
      const at = getComputedStyle(panel).translate;
      for (const moving of panel.getAnimations()) moving.cancel();
      return at === 'none' ? otherwise : at;
    };
    // With reduced motion, the panel shown takes the other's place at once.
    const timing = matchMedia('(prefers-reduced-motion: reduce)').matches
      ? { ...slide, duration: 0 }
      : slide;
    const moves = [
      // Held out once there, until it's gone.
      out.animate({ translate: [from(out, '0'), leaves] }, { ...timing, fill: 'forwards' }),
      into.animate({ translate: [from(into, arrives), '0'] }, timing),
    ];
    let live = true;
    Promise.all(moves.map((move) => move.finished)).then(
      () => live && setPanels({ shown: panels.shown, leaving: [], midway: false }),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [panels]);
  return (
    <div className="sidebar-panels">
      {[panels.shown, ...panels.leaving]
        .sort((one, other) => place(one) - place(other))
        .map((panel) => {
          const leaving = panel !== panels.shown;
          return (
            <div
              key={place(panel)}
              ref={(element) => {
                elements.current.set(panel, element!);
                return () => void elements.current.delete(panel);
              }}
              className="sidebar-panel"
              inert={leaving}
              aria-hidden={leaving || undefined}
            >
              {draw(panel)}
            </div>
          );
        })}
    </div>
  );
}
