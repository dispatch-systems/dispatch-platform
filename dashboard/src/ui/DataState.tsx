import type { ReactNode } from 'react';
import { ErrorBox } from './ErrorBox.js';
import { Loading } from './Loading.js';
import { useReadAvailability } from '../lib/read-availability.js';

/**
 * The one loading rule: no data yet means a spinner, data (even stale, beside an
 * error) means content. Pass `error` to show it above; a caller that already
 * shows it elsewhere passes `failed` instead where the spinner should stop.
 */
export function DataState<T>({
  data,
  error = '',
  failed = false,
  retry,
  children,
}: {
  data: T | undefined;
  error?: string;
  failed?: boolean;
  retry?: () => void;
  children: (data: T) => ReactNode;
}) {
  const availability = useReadAvailability();
  const failure = failed || Boolean(error);
  return (
    <>
      <ErrorBox message={error} />
      {data !== undefined ? (
        children(data)
      ) : failure ? (
        retry && <button onClick={retry}>Retry loading</button>
      ) : availability !== 'ready' ? (
        <p className="notice" role="status">
          {availability === 'offline'
            ? 'You’re offline. This page will load when you reconnect.'
            : 'Loading is paused while this tab is hidden.'}
        </p>
      ) : (
        <Loading />
      )}
    </>
  );
}
