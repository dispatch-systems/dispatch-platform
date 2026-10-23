import type { MouseEvent } from 'react';
import type { SettingsGroup } from '../../../../core/settings/api/index.js';
import { PageBadge } from '../../../../core/shell/frontend/ui/index.js';
import type { ListedPage } from '../pages.js';
import { groupLabels, pageIcon } from './icons.js';

/** A link followed as the browser would follow it, to another tab or window. */
const elsewhere = (event: MouseEvent) =>
  event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey;

/**
 * The member's pages of Settings under their groups' names, in their order, the one open marked
 * current, a page a feature puts under Settings with its badge. Each is a link to its address;
 * followed here, `open` opens it.
 */
export function PageList({
  pages,
  current,
  href,
  open,
  warm,
}: {
  pages: readonly ListedPage[];
  current: string | undefined;
  href: (page: string) => string;
  open: (page: string) => void;
  warm: (page: string) => void;
}) {
  const groups: { group: SettingsGroup; pages: ListedPage[] }[] = [];
  for (const page of pages)
    if (groups.at(-1)?.group === page.group) groups.at(-1)!.pages.push(page);
    else groups.push({ group: page.group, pages: [page] });
  return groups.map(({ group, pages }) => (
    <div
      key={group}
      className="settings-nav-group"
      role="group"
      aria-labelledby={`settings-group-${group}`}
    >
      <h3 id={`settings-group-${group}`}>{groupLabels[group]}</h3>
      {pages.map((page) => {
        const Icon = pageIcon(page);
        return (
          <a
            key={page.id}
            href={href(page.id)}
            className="settings-nav-item"
            aria-current={page.id === current ? 'page' : undefined}
            onPointerEnter={() => warm(page.id)}
            onFocus={() => warm(page.id)}
            onClick={(event) => {
              if (elsewhere(event)) return;
              event.preventDefault();
              open(page.id);
            }}
          >
            <Icon aria-hidden="true" />
            <span>{page.label}</span>
            {page.route?.badge && (
              <PageBadge badge={page.route.badge} active={page.id === current} />
            )}
          </a>
        );
      })}
    </div>
  ));
}
