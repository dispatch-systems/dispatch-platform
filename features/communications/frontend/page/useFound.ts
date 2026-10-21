import { useEffect, useState } from 'react';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';

/**
 * What `read` answers: read `wait` milliseconds after it changes, as what's typed does, and again
 * as the conversations change (`revision`), keeping the last answer until the next comes; with
 * why it failed, if it did. Nothing while there's nothing to read.
 */
export function useFound<T>(read: (() => Promise<T>) | undefined, revision: number, wait: number) {
  const [found, setFound] = useState<T>();
  const [failed, setFailed] = useState('');
  useEffect(() => {
    if (!read) {
      setFound(undefined);
      setFailed('');
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      read().then(
        (answer) => {
          if (!live) return;
          setFound(answer);
          setFailed('');
        },
        (cause) => {
          if (!live) return;
          setFound(undefined);
          setFailed(messageOf(cause));
        },
      );
    }, wait);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [read, revision, wait]);
  return { found, failed };
}
