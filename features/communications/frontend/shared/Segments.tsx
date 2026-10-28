import type { ReactNode } from 'react';
import { Tabs } from '../../../../core/shell/frontend/ui/index.js';
import './segments.css';

/** Tabs as segments of one control, in the list or above the board: as many as `items`, each as
 * wide as the others unless `className` says otherwise. */
export function Segments({
  label,
  value,
  onChange,
  items,
  className,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  items: (readonly [string, ReactNode])[];
  className?: string;
}) {
  return (
    <div className={className ? `segments ${className}` : 'segments'} data-count={items.length}>
      <Tabs label={label} value={value} onChange={onChange} items={items} />
    </div>
  );
}
