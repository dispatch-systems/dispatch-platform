import { useRef, useState } from 'react';
import { Minus, Plus } from 'lucide-react';
import type { UniformAdjustment, UniformVariant } from '../api/index.js';
import { uniformFitLabels } from '../api/index.js';
import { ApiError, view } from '../../../core/shell/frontend/runtime/api.js';
import { adjustUniform } from '../api/client.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { randomId } from '../../../core/shell/frontend/lib/random-id.js';

// A uniform's count in one fit and size, with − and + for those who may change it. Each press shows
// at once, and presses save one after another in the order made. While they do, the count is the
// one they started from and what they add, so a live update of a press already counted isn't
// counted again; once the last is done it's the live count again. One that fails is taken back
// with the presses after it, which counted on it, and the stock is read again.
export function StockCounter({
  variant,
  uniformName,
  canAdjust,
  live,
  onChange,
  refresh,
}: {
  variant: UniformVariant;
  uniformName: string;
  canAdjust: boolean;
  live: boolean;
  onChange: (change: UniformAdjustment) => void;
  refresh: () => void;
}) {
  const [pending, setPending] = useState(0);
  const [pressed, setPressed] = useState<{ from: number; ahead: number }>();
  const quantity = pressed ? pressed.from + pressed.ahead : variant.quantity;
  const queue = useRef(Promise.resolve());
  const waiting = useRef(0);
  // Which run of presses a press belongs to: a failure starts another, dropping those queued.
  const run = useRef(0);
  const action = useAction(async (delta: 1 | -1) => {
    const requestId = randomId();
    const token = view;
    setPending((n) => n + 1);
    try {
      for (let attempt = 0; ; attempt++) {
        if (view !== token) return;
        try {
          const result = await adjustUniform(
            variant.id,
            delta,
            requestId,
            AbortSignal.timeout(10_000),
          );
          if (view === token) onChange(result);
          return;
        } catch (cause) {
          // A lost acknowledgement may already have committed. Retry the same operation, never another delta.
          if (
            attempt >= 2 ||
            (cause instanceof ApiError && cause.status < 500 && cause.status !== 429)
          )
            throw cause;
          await new Promise((resolve) => setTimeout(resolve, 250 * 2 ** attempt));
        }
      }
    } catch (cause) {
      if (view === token) refresh();
      throw cause;
    } finally {
      setPending((n) => n - 1);
    }
  });
  const press = (delta: 1 | -1) => {
    waiting.current += 1;
    const from = variant.quantity;
    const of = run.current;
    setPressed((before) => ({
      from: before?.from ?? from,
      ahead: (before?.ahead ?? 0) + delta,
    }));
    queue.current = queue.current.then(async () => {
      const saved = run.current === of && (await action.run(delta));
      if (!saved && run.current === of) run.current += 1;
      waiting.current -= 1;
      const last = !waiting.current;
      setPressed((before) =>
        last || !before ? undefined : { ...before, ahead: before.ahead - (saved ? 0 : delta) },
      );
    });
  };
  const label = `${uniformName}, ${uniformFitLabels[variant.fit]}, ${variant.size}`;
  return (
    <div className="uniform-counter" aria-busy={pending > 0}>
      {canAdjust && (
        <button
          type="button"
          aria-label={`Remove one ${label}`}
          disabled={!live || quantity === 0}
          onClick={() => press(-1)}
        >
          <Minus size={14} />
        </button>
      )}
      <output aria-label={`${label} in stock`}>{quantity}</output>
      {canAdjust && (
        <button
          type="button"
          aria-label={`Add one ${label}`}
          disabled={!live || quantity >= 1_000_000}
          onClick={() => press(1)}
        >
          <Plus size={14} />
        </button>
      )}
      {pending > 0 && (
        <span className="sr-only" role="status">
          Saving inventory
        </span>
      )}
    </div>
  );
}
