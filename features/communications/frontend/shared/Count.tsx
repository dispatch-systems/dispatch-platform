import './count.css';

/** How many wait, beside a section, a tab or a workspace: nothing while none does. */
export function Count({ waiting }: { waiting: number }) {
  return waiting > 0 ? <span className="count">{waiting}</span> : null;
}
