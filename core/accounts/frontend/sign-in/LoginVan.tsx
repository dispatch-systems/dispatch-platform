import { useEffect, useRef, useState } from 'react';
import { randomVan } from './van/vans.js';

/** Mounted only on desktop. The form paints before any graphics code loads. */
export function LoginVan() {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [van] = useState(randomVan);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const abort = new AbortController();
    const element = canvas.current!;
    const timer = window.setTimeout(() => {
      void import('./van/renderer.js')
        .then(({ startVan }) => {
          if (!abort.signal.aborted) return startVan(element, van.model, abort.signal, setReady);
        })
        .catch(() => {
          if (!abort.signal.aborted) setReady(false);
        });
    }, 500);
    return () => {
      clearTimeout(timer);
      abort.abort();
    };
  }, [van]);
  return (
    <div className="login-van">
      <img src={van.poster} alt="" hidden={ready} decoding="async" />
      <canvas ref={canvas} style={{ visibility: ready ? 'visible' : 'hidden' }} />
    </div>
  );
}
