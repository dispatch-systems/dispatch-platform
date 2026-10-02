import type { ReactNode } from 'react';
import { Inbox } from 'lucide-react';

export function Empty({
  title,
  children,
  action,
}: {
  title: string;
  children?: ReactNode;
  /** What to do about it, such as a button that adds the first one. */
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      <Inbox size={28} />
      <h3>{title}</h3>
      {children && <p>{children}</p>}
      {action}
    </div>
  );
}
