import { useState, type ReactNode } from 'react';
import { Plus } from 'lucide-react';
import { Popover } from '../../../../core/shell/frontend/ui/index.js';
import './add-menu.css';

// What can be added to a message: the plus at the box's start opens the choices above it, and
// the one chosen closes it. Not there while there's nothing to add.

/** One of the choices: what it says, its icon, and what choosing it does. */
export type Adding = { label: string; icon: ReactNode; choose: () => void; disabled?: boolean };

export function AddMenu({ disabled, choices }: { disabled: boolean; choices: Adding[] }) {
  const [open, setOpen] = useState(false);
  if (!choices.length) return null;
  return (
    <Popover
      className="add-menu"
      triggerClassName="round"
      panelClassName="add-menu-panel"
      label="Add"
      trigger={<Plus size={18} />}
      open={open}
      disabled={disabled}
      onRequestOpen={() => setOpen(true)}
      onRequestClose={() => setOpen(false)}
    >
      <ul>
        {choices.map((choice) => (
          <li key={choice.label}>
            <button
              type="button"
              disabled={choice.disabled}
              onClick={() => {
                setOpen(false);
                choice.choose();
              }}
            >
              {choice.icon}
              {choice.label}
            </button>
          </li>
        ))}
      </ul>
    </Popover>
  );
}
