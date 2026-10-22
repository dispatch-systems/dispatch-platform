import type { LucideIcon } from 'lucide-react';

/** One way a page can show its items: its value, its name and its icon. */
export interface ViewOption<T extends string> {
  value: T;
  label: string;
  icon: LucideIcon;
}

/** How a page shows its items, as a list or a grid: icons in one group, the one in use pressed. */
export function ViewSwitch<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: readonly ViewOption<T>[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="view-switch" role="group" aria-label="View">
      {options.map(({ value: option, label, icon: Icon }) => (
        <button
          key={option}
          aria-pressed={value === option}
          aria-label={label}
          onClick={() => onChange(option)}
        >
          <Icon size={16} aria-hidden="true" />
        </button>
      ))}
    </div>
  );
}
