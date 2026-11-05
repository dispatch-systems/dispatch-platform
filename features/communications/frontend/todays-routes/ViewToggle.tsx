import { List, Table } from 'lucide-react';
import './view-toggle.css';

// How Today's Routes shows: a list beside the conversation open, or a board across the page.

export type RoutesView = 'list' | 'board';

export function ViewToggle({
  view,
  choose,
}: {
  view: RoutesView;
  choose: (view: RoutesView) => void;
}) {
  return (
    <div className="view-toggle" role="group" aria-label="View">
      <button aria-pressed={view === 'list'} onClick={() => choose('list')}>
        <List aria-hidden="true" />
        List
      </button>
      <button aria-pressed={view === 'board'} onClick={() => choose('board')}>
        <Table aria-hidden="true" />
        Board
      </button>
    </div>
  );
}
