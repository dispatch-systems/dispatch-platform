import type { ReactNode } from 'react';

export function Tabs({
  value,
  onChange,
  items,
  label,
  onIntent,
}: {
  value: string;
  onChange: (value: string) => void;
  /** Each tab's id and its label. */
  items: (readonly [string, ReactNode] | string[])[];
  label: string;
  /** Optional code/data warming for a deliberate next-tab selection. */
  onIntent?: (value: string) => void;
}) {
  return (
    <div className="restored-tabs" role="tablist" aria-label={label}>
      {items.map(([id, text], index) => (
        <button
          key={id}
          type="button"
          role="tab"
          aria-selected={value === id}
          tabIndex={value === id ? 0 : -1}
          onPointerEnter={() => onIntent?.(id!)}
          onPointerDown={() => onIntent?.(id!)}
          onFocus={() => onIntent?.(id!)}
          onKeyDown={(event) => {
            const offset = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
            if (!offset && !['Home', 'End'].includes(event.key)) return;
            event.preventDefault();
            const next =
              event.key === 'Home'
                ? 0
                : event.key === 'End'
                  ? items.length - 1
                  : (index + offset + items.length) % items.length;
            onChange(items[next]![0]!);
            (event.currentTarget.parentElement?.children[next] as HTMLElement)?.focus();
          }}
          onClick={() => onChange(id!)}
        >
          {text}
        </button>
      ))}
    </div>
  );
}
