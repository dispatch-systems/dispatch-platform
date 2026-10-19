import { useEffect, useRef, type KeyboardEvent } from 'react';
import { Search, X } from 'lucide-react';

/** Whether keys typed at `target` are text being written. */
const writing = (target: EventTarget | null) =>
  target instanceof HTMLElement &&
  (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName));

/**
 * The search of Settings' pages: a field over the list of what it finds (`list`, its option
 * `active` the one Enter opens), with a way to clear it. "/" anywhere else on the page, but in a
 * dialog over it, moves here; Escape clears it.
 */
export function SearchField({
  query,
  change,
  list,
  active,
  move,
  choose,
}: {
  query: string;
  change: (query: string) => void;
  /** The id of the list of what it found, while it found something. */
  list: string | undefined;
  /** The id of the option Enter opens. */
  active: string | undefined;
  /** Moves through the options, by `step`. */
  move: (step: number) => void;
  /** Opens the active option. */
  choose: () => void;
}) {
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const slash = (event: globalThis.KeyboardEvent) => {
      const input = field.current;
      const dialog = document.querySelector('[aria-modal="true"], dialog[open]');
      if (
        event.key !== '/' ||
        event.defaultPrevented ||
        event.ctrlKey ||
        event.metaKey ||
        event.altKey ||
        writing(event.target) ||
        !input ||
        (dialog && !dialog.contains(input))
      )
        return;
      event.preventDefault();
      input.focus();
    };
    window.addEventListener('keydown', slash);
    return () => window.removeEventListener('keydown', slash);
  }, []);
  const key = (event: KeyboardEvent) => {
    const steps: Record<string, number> = { ArrowDown: 1, ArrowUp: -1 };
    if (event.key in steps && list) move(steps[event.key]!);
    else if (event.key === 'Enter' && active) choose();
    else if (event.key === 'Escape' && query) change('');
    else return;
    // Handled here: Escape leaves the navigation open over a narrow page.
    event.preventDefault();
  };
  return (
    <div className="settings-find">
      <Search aria-hidden="true" />
      <input
        ref={field}
        type="text"
        role="combobox"
        aria-label="Search settings"
        aria-autocomplete="list"
        aria-expanded={Boolean(list)}
        aria-controls={list}
        aria-activedescendant={active}
        placeholder="Search"
        autoComplete="off"
        spellCheck={false}
        enterKeyHint="search"
        value={query}
        onChange={(event) => change(event.target.value)}
        onKeyDown={key}
      />
      {query && (
        <button
          type="button"
          className="settings-find-clear"
          aria-label="Clear search"
          onClick={() => {
            change('');
            field.current?.focus();
          }}
        >
          <X aria-hidden="true" />
        </button>
      )}
    </div>
  );
}
