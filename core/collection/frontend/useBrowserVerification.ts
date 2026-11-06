import { useCallback, useEffect, useRef, useState } from 'react';
import type { BrowserFrame, BrowserInput, Connection } from '../api/index.js';
import { api, ApiError } from '../../shell/frontend/runtime/api.js';
import { messageOf } from '../../shell/frontend/lib/errors.js';
import { connectionUrl } from '../api/client.js';

type Point = { x: number; y: number };

export function useBrowserVerification({
  sessionId,
  close,
  provider,
}: {
  sessionId: string;
  provider: Connection['provider'];
  close: () => void;
}) {
  const endpoint = connectionUrl(provider);
  const [frame, setFrame] = useState<BrowserFrame>();
  const [error, setError] = useState('');
  const [streamError, setStreamError] = useState('');
  const [busy, setBusy] = useState(false);
  const [expired, setExpired] = useState(false);
  const controller = useRef<AbortController | null>(null);
  const pending = useRef<Promise<unknown>>(Promise.resolve());
  const capturing = useRef(false);
  const pressed = useRef(false);
  const blocked = useRef(false);
  const move = useRef<Extract<BrowserInput, { kind: 'pointer' }> | undefined>(undefined);
  const moveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  blocked.current = busy || expired;

  const failed = useCallback((error: Error) => {
    if (controller.current?.signal.aborted || error.name === 'AbortError') return;
    setError(messageOf(error));
    if (
      error instanceof ApiError &&
      ['verification_expired', 'permission_denied'].includes(error.code)
    ) {
      setExpired(true);
      blocked.current = true;
    }
  }, []);
  const capture = useCallback(async () => {
    if (capturing.current || blocked.current || controller.current?.signal.aborted) return;
    capturing.current = true;
    const signal = controller.current?.signal;
    try {
      const next = await api<BrowserFrame>(
        `${endpoint}/screenshot?sessionId=${encodeURIComponent(sessionId)}`,
        undefined,
        signal,
      );
      if (signal?.aborted) return;
      if (next.sessionId !== sessionId)
        throw new ApiError('verification_expired', 'This verification session has ended.', 409);
      setFrame(next);
      setStreamError('');
    } catch (error) {
      if (!signal?.aborted && (error as Error).name !== 'AbortError') {
        setStreamError(messageOf(error));
        if (error instanceof ApiError && [401, 403, 409].includes(error.status)) failed(error);
      }
    } finally {
      capturing.current = false;
    }
  }, [sessionId, failed, endpoint]);
  useEffect(() => {
    const abort = new AbortController();
    controller.current = abort;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      await capture();
      if (!abort.signal.aborted) timer = setTimeout(() => void poll(), 650);
    };
    void poll();
    return () => {
      abort.abort();
      clearTimeout(timer);
      clearTimeout(moveTimer.current);
    };
  }, [capture]);
  const send = (input: BrowserInput) => {
    if (blocked.current || controller.current?.signal.aborted) return;
    const abort = controller.current;
    pending.current = pending.current
      .then(async () => {
        if (abort?.signal.aborted) return;
        await api(`${endpoint}/assist`, { sessionId, input }, abort?.signal);
      })
      .catch(failed);
  };
  const flushMove = () => {
    clearTimeout(moveTimer.current);
    moveTimer.current = undefined;
    if (move.current) {
      send(move.current);
      move.current = undefined;
    }
  };
  const pointerDown = (point: Point) => {
    flushMove();
    pressed.current = true;
    send({ kind: 'pointer', phase: 'down', pressed: true, ...point });
  };
  const pointerMove = (point: Point) => {
    move.current = { kind: 'pointer', phase: 'move', pressed: pressed.current, ...point };
    if (!moveTimer.current) {
      const abort = controller.current;
      moveTimer.current = setTimeout(() => {
        const scheduled = moveTimer.current;
        // Keep the latest position while earlier input is in flight.
        // A slow connection must not accumulate a trail of old moves.
        void pending.current.then(() => {
          if (moveTimer.current === scheduled && !abort?.signal.aborted && !blocked.current)
            flushMove();
        });
      }, 60);
    }
  };
  const pointerUp = (point: Point) => {
    if (!pressed.current) return;
    flushMove();
    pressed.current = false;
    send({ kind: 'pointer', phase: 'up', pressed: false, ...point });
  };
  const submit = async () => {
    flushMove();
    blocked.current = true;
    setBusy(true);
    setError('');
    try {
      await pending.current;
      if (controller.current?.signal.aborted) return;
      await api(`${endpoint}/submit`, { sessionId }, controller.current?.signal);
      close();
    } catch (error) {
      failed(error as Error);
    } finally {
      if (!controller.current?.signal.aborted) {
        setBusy(false);
        blocked.current = false;
      }
    }
  };
  return {
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
    inputBlocked: () => blocked.current,
    pointerPressed: () => pressed.current,
  };
}
