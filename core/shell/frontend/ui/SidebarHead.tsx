import { ArrowLeft } from 'lucide-react';

/** The top of a sidebar that stands in for another: the way back to that one, and its name. */
export function SidebarHead({ title, back }: { title: string; back: () => void }) {
  return (
    <div className="sidebar-head">
      <button type="button" className="icon-button" aria-label="Back" onClick={back}>
        <ArrowLeft aria-hidden="true" />
      </button>
      <h2>{title}</h2>
    </div>
  );
}
