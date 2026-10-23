import { useEffect, useState, type ComponentType } from 'react';
import type { Route } from '../runtime/slots.js';

type Badge = NonNullable<Route['badge']>;
const badges = new WeakMap<Badge, ComponentType<{ active: boolean }>>();

/**
 * A page's badge beside its name in a sidebar's list, once its code has loaded; one that fails to
 * load is left out. `active` while the page is open.
 */
export function PageBadge({ badge, active }: { badge: Badge; active: boolean }) {
  const [Drawn, setDrawn] = useState(() => badges.get(badge));
  useEffect(() => {
    if (Drawn) return;
    let live = true;
    badge.load().then(
      (loaded) => {
        badges.set(badge, loaded);
        if (live) setDrawn(() => loaded);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [badge, Drawn]);
  return Drawn ? (
    <span className="nav-badge">
      <Drawn active={active} />
    </span>
  ) : null;
}
