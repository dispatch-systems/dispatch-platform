import { useEffect, useRef, useState } from 'react';
import type { PlaygroundStatus } from '../../../../shared/contracts/index.js';
import { getPlaygroundAccess, getPlaygroundStatus, startPlayground } from '../../app/endpoints.js';
import { useAction } from '../../app/useAction.js';
import { Empty, ErrorBox, Loading } from '../../ui/index.js';
export function DesignPlaygroundPage() {
  const [status, setStatus] = useState<PlaygroundStatus>();
  const [canvasMounted, setCanvasMounted] = useState(false);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  const revision = useRef(0);
  const start = useAction(
    async () => {
      revision.current++;
      try {
        const next = await startPlayground();
        setStatus(next);
        if (next.running) setCanvasMounted(true);
      } finally {
        revision.current++;
      }
    },
    { inline: true },
  );
  useEffect(() => {
    let active = true;
    let timer: number | undefined;
    async function check() {
      const expected = revision.current;
      try {
        const next = await getPlaygroundStatus();
        if (active && expected === revision.current) {
          setStatus(next);
          if (next.running) setCanvasMounted(true);
          setError('');
        }
      } catch (cause) {
        if (active && expected === revision.current)
          setError(cause instanceof Error ? cause.message : 'Unable to check the playground.');
      } finally {
        if (active) timer = window.setTimeout(() => void check(), 10_000);
      }
    }
    void check();
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [retry]);
  if (!status && !error) return <Loading />;
  const recovery = (
    <div className="design-playground-status">
      <Empty
        title={
          status?.configured
            ? 'Design Playground is offline'
            : 'Design Playground is not available yet'
        }
      />
      {(start.error || error) && <ErrorBox message={start.error || error} />}
      <div className="design-playground-actions">
        {status?.canStart && (
          <button className="button primary" disabled={start.busy} onClick={() => void start.run()}>
            {start.busy ? 'Starting playground…' : 'Start playground'}
          </button>
        )}
        <button className="button" disabled={start.busy} onClick={() => setRetry((old) => old + 1)}>
          Try again
        </button>
      </div>
    </div>
  );
  if (!canvasMounted) return recovery;
  return (
    <div className="design-playground-shell">
      <PlaygroundCanvas />
      {(!status?.running || error) && (
        <div className="design-playground-recovery" role="region" aria-label="Playground recovery">
          {recovery}
        </div>
      )}
    </div>
  );
}

function PlaygroundCanvas() {
  const frame = useRef<HTMLIFrameElement>(null);
  const [origin, setOrigin] = useState<string | null | undefined>(undefined);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  const requestAccess = useRef<() => void>(() => {});
  useEffect(() => {
    let active = true,
      loading = false,
      issued = 0;
    let retryTimer: number | undefined;
    let currentOrigin: string | null = null;
    let ticket: string | null = null;
    const send = () => {
      if (!currentOrigin || !ticket || !frame.current?.contentWindow) return false;
      frame.current?.contentWindow?.postMessage(
        { type: 'playground:access', ticket },
        currentOrigin,
      );
      frame.current?.contentWindow?.postMessage(
        {
          type: 'playground:theme',
          theme: document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light',
        },
        currentOrigin,
      );
      ticket = null;
      return true;
    };
    const refresh = async () => {
      if (!active) return;
      if (loading) {
        send();
        return;
      }
      const wait = 1500 - (Date.now() - issued);
      if (wait > 0) {
        if (!send() && retryTimer === undefined) {
          retryTimer = window.setTimeout(() => {
            retryTimer = undefined;
            void refresh();
          }, wait);
        }
        return;
      }
      window.clearTimeout(retryTimer);
      retryTimer = undefined;
      loading = true;
      try {
        const access = await getPlaygroundAccess();
        if (!active) return;
        currentOrigin = access.origin;
        ticket = access.ticket;
        issued = Date.now();
        setOrigin(access.origin);
        setError('');
        send();
      } catch (cause) {
        if (active) {
          setError(cause instanceof Error ? cause.message : 'Unable to open Design Playground.');
          setOrigin(undefined);
        }
      } finally {
        loading = false;
      }
    };
    const message = (event: MessageEvent) => {
      if (event.source !== frame.current?.contentWindow || event.origin !== currentOrigin) return;
      if (event.data?.type === 'playground:ready') void refresh();
    };
    requestAccess.current = () => {
      if (ticket) send();
      else void refresh();
    };
    window.addEventListener('message', message);
    const appearance = new MutationObserver(() => {
      if (currentOrigin)
        frame.current?.contentWindow?.postMessage(
          {
            type: 'playground:theme',
            theme: document.documentElement.dataset.theme,
          },
          currentOrigin,
        );
    });
    appearance.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });
    void refresh();
    return () => {
      active = false;
      window.clearTimeout(retryTimer);
      appearance.disconnect();
      window.removeEventListener('message', message);
      requestAccess.current = () => {};
    };
  }, [retry]);
  if (error)
    return (
      <div className="design-playground-status">
        <ErrorBox message={error} />
        <button className="button" onClick={() => setRetry((old) => old + 1)}>
          Try again
        </button>
      </div>
    );
  if (origin === undefined) return <Loading />;
  if (origin === null) return <Empty title="Design Playground is not available yet" />;
  return (
    <div className="design-playground-page">
      <iframe
        ref={frame}
        src={origin}
        title="Design Playground"
        className="design-playground-frame"
        sandbox="allow-scripts allow-same-origin allow-forms"
        referrerPolicy="no-referrer"
        onLoad={() => requestAccess.current()}
      />
    </div>
  );
}
