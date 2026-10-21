import { useEffect, useState } from 'react';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';

/**
 * What `read` answers: read `wait` milliseconds after it changes, as what's typed does, and again
 * as the conversations change (`revision`); with why it failed, if it did. The last answer stays
 * until the next comes while it's of the same list (`list`, as one workspace's contacts found by
 * a name), and goes at once for another. Nothing while there's nothing to read.
 */
export function useFound<T>(
  read: (() => Promise<T>) | undefined,
  list: string,
  revision: number,
  wait: number,
) {
  const [last, setLast] = useState<{ list: string; found?: T; failed: string }>();
  useEffect(() => {
    if (!read) return;
    let live = true;
    const timer = setTimeout(() => {
      read().then(
        (found) => live && setLast({ list, found, failed: '' }),
        (cause) => live && setLast({ list, failed: messageOf(cause) }),
      );
    }, wait);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [read, list, revision, wait]);
  const shown = read && last?.list === list ? last : undefined;
  return { found: shown?.found, failed: shown?.failed ?? '' };
}
