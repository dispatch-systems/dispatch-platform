import type { ReactNode } from 'react';
import type { Driver } from '../api/index.js';
import { DriverAvatar } from './DriverAvatar.js';

/** A person's avatar and name, opening their details. */
export function DriverButton({
  driver,
  children,
  onOpen,
}: {
  driver: Driver;
  children?: ReactNode;
  onOpen: (code: string) => void;
}) {
  return (
    <button type="button" className="driver-person" onClick={() => onOpen(driver.code)}>
      <DriverAvatar name={driver.name} code={driver.code} />
      <div>
        <strong>{driver.name}</strong>
        {children}
      </div>
    </button>
  );
}
