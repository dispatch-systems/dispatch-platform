import { useEffect, useRef, useState } from 'react';
import { Search } from 'lucide-react';
import type { GifSearch, MessageGif } from '../../api/index.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { findGifs, shownAt } from './klipy.js';
import '../shared/picker.css';
import './gif-picker.css';

/** How long typing rests before it's searched. */
const TYPING_MS = 300;

/**
 * KLIPY's GIFs over the thread, where the emoji picker opens: trending ones until something is
 * typed, then those found for it, in KLIPY's order. The one chosen is `pick`ed; Escape or a press
 * outside closes it.
 */
export function GifPicker({
  search,
  pick,
  close,
}: {
  search: GifSearch;
  pick: (gif: MessageGif) => void;
  close: () => void;
}) {
  const { fail } = useFeedback();
  const [typed, setTyped] = useState('');
  const [gifs, setGifs] = useState<MessageGif[]>();
  const panel = useRef<HTMLDivElement>(null);
  const closing = useRef(close);
  const failing = useRef(fail);
  useEffect(() => {
    closing.current = close;
    failing.current = fail;
  }, [close, fail]);
  useEffect(() => {
    const outside = (event: PointerEvent) => {
      if (!panel.current?.contains(event.target as Node)) closing.current();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closing.current();
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('keydown', escape);
    };
  }, []);
  useEffect(() => {
    const controller = new AbortController();
    const timer = window.setTimeout(
      () => {
        findGifs(search, typed, controller.signal).then(setGifs, () => {
          if (!controller.signal.aborted) failing.current("Couldn't load GIFs.");
        });
      },
      typed ? TYPING_MS : 0,
    );
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [search, typed]);
  const columns = [0, 1].map((column) => (gifs ?? []).filter((_, n) => n % 2 === column));
  return (
    <div ref={panel} className="gif-panel" role="dialog" aria-label="Choose GIF">
      <label className="gif-search">
        <Search size={15} aria-hidden="true" />
        <input
          autoFocus
          type="search"
          aria-label="Search KLIPY"
          placeholder="Search KLIPY"
          value={typed}
          onChange={(event) => setTyped(event.target.value)}
          onKeyDown={(event) => {
            // Within what's written's form, Enter searches rather than sending.
            if (event.key === 'Enter') event.preventDefault();
          }}
        />
      </label>
      <div className="gif-grid">
        {columns.map((column, n) => (
          <div key={n} className="gif-column">
            {column.map((gif) => {
              const tile = shownAt(gif, 160);
              return (
                <button
                  key={gif.slug}
                  type="button"
                  aria-label={gif.title || 'GIF'}
                  style={{ aspectRatio: `${tile.width} / ${tile.height}` }}
                  onClick={() => pick(gif)}
                >
                  <img src={tile.url} alt="" loading="lazy" />
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
