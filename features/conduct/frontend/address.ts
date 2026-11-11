import { useCallback, useEffect, useState } from 'react';
import { emptyFilters, type HistoryFilters } from '../api/client.js';
import { leaving, type Guards } from '../../../core/shell/frontend/runtime/leave.js';
import { holdAddress } from '../../../core/shell/frontend/runtime/holds.js';
import {
  hashQuery,
  pushHashQuery,
  replaceHashQuery,
} from '../../../core/shell/frontend/runtime/navigation.js';

// What the page shows, kept in its address so Back, a reload and a link each open it again:
// `?tab=writeups`, `?driver=<code>`, `?writeup=<id>`, `?write=new` (with `for=<code>`) and
// `?write=<id>` to edit one.

export type ConductAddress =
  | { view: 'list'; tab: 'drivers' | 'writeups'; filters?: HistoryFilters }
  | { view: 'driver'; person: string }
  | { view: 'writeup'; id: string }
  | { view: 'write'; edit?: string; person?: string };

function read(): ConductAddress {
  const query = hashQuery();
  const write = query.get('write');
  if (write)
    return {
      view: 'write',
      edit: write === 'new' ? undefined : write,
      person: query.get('for') ?? undefined,
    };
  const writeup = query.get('writeup');
  if (writeup) return { view: 'writeup', id: writeup };
  const driver = query.get('driver');
  if (driver) return { view: 'driver', person: driver };
  return {
    view: 'list',
    tab: query.get('tab') === 'writeups' ? 'writeups' : 'drivers',
    filters: {
      search: query.get('q') ?? '',
      category: query.get('category') ?? '',
      tier: query.get('tier') ?? '',
      status: query.get('status') ?? '',
    },
  };
}

function query(address: ConductAddress): Record<string, string> {
  switch (address.view) {
    case 'list':
      return Object.fromEntries(
        Object.entries({
          tab: address.tab === 'writeups' ? 'writeups' : '',
          q: address.filters?.search ?? '',
          category: address.filters?.category ?? '',
          tier: address.filters?.tier ?? '',
          status: address.filters?.status ?? '',
        }).filter(([, value]) => value),
      );
    case 'driver':
      return { driver: address.person };
    case 'writeup':
      return { writeup: address.id };
    case 'write':
      return {
        write: address.edit ?? 'new',
        ...(address.person && !address.edit ? { for: address.person } : {}),
      };
  }
}

/** What a step of the browser's history Conduct made itself holds, so Back stays on the page. */
const OWN = 'conduct';
const own = () => (history.state as Record<string, unknown> | null)?.[OWN] === true;

/**
 * What the address says the page shows; `open`, which shows another as a step of the browser's
 * history, or in place of this one with `replace`; and `back`, which goes back a step while the
 * page made it, or else to the list.
 */
export function useConductAddress(guards: Guards) {
  const [address, setAddress] = useState(read);
  useEffect(() => {
    const follow = () => setAddress(read());
    window.addEventListener('hashchange', follow);
    return () => {
      window.removeEventListener('hashchange', follow);
    };
  }, []);
  useEffect(() => holdAddress((then) => leaving(guards, then)), [guards]);
  const open = useCallback(
    (next: ConductAddress, { replace = false, saved = false } = {}) => {
      const move = () => {
        if (!replace) history.replaceState({ ...history.state, conductScroll: window.scrollY }, '');
        if (replace) {
          history.replaceState({ ...history.state, conductScroll: undefined }, '');
          replaceHashQuery(query(next));
        } else pushHashQuery(query(next), { [OWN]: true });
        setAddress(next);
        if (!replace) window.scrollTo(0, 0);
      };
      if (saved) move();
      else leaving(guards, move);
    },
    [guards],
  );
  const back = useCallback(() => {
    if (own()) history.back();
    else open({ view: 'list', tab: 'drivers' }, { replace: true });
  }, [open]);
  return {
    address,
    open,
    back,
    filters: address.view === 'list' ? (address.filters ?? emptyFilters) : emptyFilters,
  };
}
