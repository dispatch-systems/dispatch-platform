import { useEffect, useRef, useState } from 'react';
import { getPlaygroundAccess } from '../../app/endpoints.js';
import { Empty, ErrorBox, Loading } from '../../ui/index.js';
export function DesignPlaygroundPage() {
  const frame = useRef<HTMLIFrameElement>(null);
  const [origin, setOrigin] = useState<string | null | undefined>(undefined);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  const requestAccess = useRef<() => void>(() => {});
  useEffect(() => {
    let active = true,
      loading = false,
      issued = 0;
    let currentOrigin: string | null = null;
    let ticket: string | null = null;
    const send = () => {
      if (!currentOrigin || !ticket || !frame.current?.contentWindow) return;
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
    };
    const refresh = async () => {
      if (loading || Date.now() - issued < 1500) {
        send();
        return;
      }
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
  if (origin === null)
    return (
      <Empty title="Design Playground is not available yet">
        Your design workspace is still being set up.
      </Empty>
    );
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
