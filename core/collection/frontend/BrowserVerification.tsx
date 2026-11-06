import { useEffect, useRef, useState, type PointerEvent } from 'react';
import { Maximize2, Minimize2, RefreshCw } from 'lucide-react';
import type { BrowserInput, Connection } from '../api/index.js';
import { featureLabel } from '../../shell/frontend/runtime/features.js';
import { ErrorBox, Loading, Modal } from '../../shell/frontend/ui/index.js';
import { useBrowserVerification } from './useBrowserVerification.js';

export function BrowserVerification({
  sessionId,
  close,
  provider,
}: {
  sessionId: string;
  provider: Connection['provider'];
  close: () => void;
}) {
  const name = featureLabel(provider);
  const {
    frame,
    error,
    streamError,
    busy,
    expired,
    capture,
    send,
    submit,
    pointerDown,
    pointerMove,
    pointerUp,
    inputBlocked,
    pointerPressed,
  } = useBrowserVerification({ sessionId, provider, close });
  const [zoom, setZoom] = useState(false);
  const [text, setText] = useState('');
  const image = useRef<HTMLImageElement>(null);
  const screen = useRef<HTMLDivElement>(null);
  const submitButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const viewport = screen.current;
    const preventPageScroll = (event: WheelEvent) => {
      if (event.target === image.current) event.preventDefault();
    };
    viewport?.addEventListener('wheel', preventPageScroll, { passive: false });
    return () => viewport?.removeEventListener('wheel', preventPageScroll);
  }, [capture]);
  useEffect(() => {
    if (zoom && screen.current) {
      screen.current.scrollLeft = (screen.current.scrollWidth - screen.current.clientWidth) / 2;
      screen.current.scrollTop = Math.max(
        0,
        (screen.current.scrollHeight - screen.current.clientHeight) / 2,
      );
    }
  }, [zoom]);
  const position = (element: HTMLImageElement, clientX: number, clientY: number) => {
    const rect = element.getBoundingClientRect();
    return {
      x: Math.max(
        0,
        Math.min(
          element.naturalWidth - 1,
          ((clientX - rect.left) * element.naturalWidth) / rect.width,
        ),
      ),
      y: Math.max(
        0,
        Math.min(
          element.naturalHeight - 1,
          ((clientY - rect.top) * element.naturalHeight) / rect.height,
        ),
      ),
    };
  };
  const release = (event: PointerEvent<HTMLImageElement>) => {
    if (!pointerPressed()) return;
    pointerUp(position(event.currentTarget, event.clientX, event.clientY));
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  };
  return (
    <Modal title={`Complete ${name} verification`} variant="browser" onClose={close}>
      <p className="verification-instructions">
        Complete the verification requested in the window below, then click Submit to continue
        signing in.
      </p>
      <div className="verification-toolbar">
        <span className="muted" role="status">
          {busy
            ? 'Continuing login…'
            : expired
              ? 'Session ended'
              : streamError
                ? 'Reconnecting…'
                : `Live ${name} window`}
        </span>
        <div>
          <button
            type="button"
            disabled={!frame || busy}
            onClick={() => setZoom((value) => !value)}
          >
            {zoom ? <Minimize2 size={15} /> : <Maximize2 size={15} />}
            {zoom ? 'Fit window' : 'Zoom in'}
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label="Refresh browser window"
            disabled={busy || expired}
            onClick={() => void capture()}
          >
            <RefreshCw size={16} />
          </button>
        </div>
      </div>
      <div ref={screen} className={`verification-screen ${zoom ? 'zoomed' : ''}`} aria-busy={busy}>
        {!frame ? (
          <Loading />
        ) : (
          <img
            ref={image}
            className="verification-browser"
            alt={`Interactive ${name} verification browser`}
            tabIndex={busy || expired ? -1 : 0}
            role="application"
            aria-label={`${name} verification browser`}
            aria-describedby="verification-keyboard-help"
            draggable={false}
            src={`data:image/png;base64,${frame.image}`}
            onLoad={() => {
              if (zoom && screen.current && !pointerPressed() && screen.current.scrollLeft === 0)
                screen.current.scrollLeft =
                  (screen.current.scrollWidth - screen.current.clientWidth) / 2;
            }}
            onContextMenu={(event) => event.preventDefault()}
            onPointerDown={(event) => {
              if (inputBlocked() || event.button !== 0 || !event.isPrimary) return;
              event.preventDefault();
              event.currentTarget.focus();
              event.currentTarget.setPointerCapture(event.pointerId);
              pointerDown(position(event.currentTarget, event.clientX, event.clientY));
            }}
            onPointerMove={(event) => {
              if (inputBlocked() || !event.isPrimary) return;
              pointerMove(position(event.currentTarget, event.clientX, event.clientY));
            }}
            onPointerUp={release}
            onPointerCancel={release}
            onLostPointerCapture={release}
            onWheel={(event) => {
              if (inputBlocked()) return;
              const scale = event.deltaMode === 1 ? 20 : event.deltaMode === 2 ? 500 : 1;
              send({
                kind: 'scroll',
                ...position(event.currentTarget, event.clientX, event.clientY),
                deltaX: Math.max(-2000, Math.min(2000, event.deltaX * scale)),
                deltaY: Math.max(-2000, Math.min(2000, event.deltaY * scale)),
              });
            }}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.preventDefault();
                event.stopPropagation();
                submitButton.current?.focus();
                return;
              }
              if (inputBlocked() || event.ctrlKey || event.metaKey || event.altKey) return;
              if (event.key.length === 1) {
                event.preventDefault();
                event.stopPropagation();
                send({ kind: 'type', text: event.key });
              } else if (
                [
                  'Enter',
                  'Tab',
                  'Backspace',
                  'Delete',
                  'ArrowDown',
                  'ArrowUp',
                  'ArrowLeft',
                  'ArrowRight',
                  'Home',
                  'End',
                  'PageUp',
                  'PageDown',
                ].includes(event.key)
              ) {
                event.preventDefault();
                event.stopPropagation();
                send({
                  kind: 'key',
                  key: event.key as Extract<BrowserInput, { kind: 'key' }>['key'],
                  shift: event.shiftKey,
                });
              }
            }}
            onPaste={(event) => {
              event.preventDefault();
              const value = event.clipboardData.getData('text');
              if (value) send({ kind: 'type', text: value.slice(0, 256) });
            }}
          />
        )}
      </div>
      <div className="verification-footer">
        <p id="verification-keyboard-help" className="muted">
          Click, drag, scroll, or type in the window. Press Escape to return to these controls.
        </p>
        <form
          className="verification-text"
          onSubmit={(event) => {
            event.preventDefault();
            if (text) {
              send({ kind: 'type', text });
              setText('');
            }
          }}
        >
          <input
            aria-label={`Text to type in ${name}`}
            placeholder="Text to type in the selected field"
            value={text}
            maxLength={256}
            autoComplete="off"
            disabled={busy || expired || !frame}
            onChange={(event) => setText(event.target.value)}
          />
          <button type="submit" disabled={busy || expired || !text}>
            Type
          </button>
        </form>
        <ErrorBox message={error || streamError} />
        <div className="form-actions">
          <button type="button" onClick={close}>
            Close
          </button>
          <button
            ref={submitButton}
            type="button"
            className="primary"
            disabled={busy || expired || !frame}
            onClick={() => void submit()}
          >
            {busy ? 'Continuing…' : 'Submit'}
          </button>
        </div>
      </div>
    </Modal>
  );
}
