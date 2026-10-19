import { Fragment } from 'react';
import type { Found, Marked } from '../search/search.js';
import { pageIcon } from './icons.js';

/** The option a result is, by the list's id and its page's. */
export const resultId = (list: string, page: string) => `${list}-${page}`;

/** Text with what the search matched in it marked. */
function Marks({ text }: { text: Marked }) {
  return text.map((part, at) =>
    part.mark ? <mark key={at}>{part.text}</mark> : <Fragment key={at}>{part.text}</Fragment>,
  );
}

/**
 * The pages a search found, best first, each with what on it matched: the options of the search
 * field's list, `active` the one Enter opens. The one open is marked current.
 */
export function Results({
  id,
  found,
  active,
  current,
  open,
  warm,
}: {
  id: string;
  found: readonly Found[];
  active: number;
  current: string | undefined;
  open: (found: Found) => void;
  warm: (page: string) => void;
}) {
  return (
    <ul id={id} role="listbox" aria-label="Found" className="settings-results">
      {found.map((each, at) => {
        const Icon = pageIcon(each.page);
        return (
          <li
            key={each.page.id}
            id={resultId(id, each.page.id)}
            role="option"
            aria-selected={at === active}
            aria-current={each.page.id === current ? 'page' : undefined}
            className="settings-nav-item settings-result"
            // The search field keeps the focus, and with it the keys that move through these.
            onMouseDown={(event) => event.preventDefault()}
            onPointerEnter={() => warm(each.page.id)}
            onClick={() => open(each)}
          >
            <Icon aria-hidden="true" />
            <span>
              <span className="settings-result-name">
                <Marks text={each.name} />
              </span>
              {each.what && (
                <small>
                  <Marks text={each.what} />
                </small>
              )}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
