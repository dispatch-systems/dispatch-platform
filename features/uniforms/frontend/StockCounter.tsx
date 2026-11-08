import { useRef, useState } from 'react';
import { Minus, Plus } from 'lucide-react';
import type { UniformAdjustment, UniformVariant } from '../api/index.js';
import { uniformFitLabels } from '../api/index.js';
import { ApiError, view } from '../../../core/shell/frontend/runtime/api.js';
import { adjustUniform } from '../api/client.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { randomId } from '../../../core/shell/frontend/lib/random-id.js';

// A uniform's count in one fit and size, with − and + for those who may change it. Each press shows
// at once, and presses save one after another in the order made, so the count never jumps back;
// one that fails is taken back and the stock read again.
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
  // The presses not yet saved, over the count as last answered.
  const [ahead, setAhead] = useState(0);
  const quantity = variant.quantity + ahead;
  const queue = useRef(Promise.resolve());
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
      setAhead((n) => n - delta);
    }
  });
  const press = (delta: 1 | -1) => {
    setAhead((n) => n + delta);
    queue.current = queue.current.then(async () => {
      await action.run(delta);
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
