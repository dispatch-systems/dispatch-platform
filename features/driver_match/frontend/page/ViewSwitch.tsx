import { LayoutGrid, List } from 'lucide-react';
import { useState } from 'react';

/** How the page shows people: a row each, or a card each. */
export type View = 'list' | 'cards';

/** Where the browser keeps the choice, as Documents keeps its layout. */
const VIEW = 'driver-match.view';

/** How the page shows people, a list until someone chooses cards, and the way to choose. */
export function useView(): [View, (view: View) => void] {
  const [view, setView] = useState<View>(() =>
    localStorage.getItem(VIEW) === 'cards' ? 'cards' : 'list',
  );
  const choose = (next: View) => {
    setView(next);
    localStorage.setItem(VIEW, next);
  };
  return [view, choose];
}

/** List or Cards, across from the page's name. */
export function ViewSwitch({ view, onChange }: { view: View; onChange: (view: View) => void }) {
  return (
    <div className="driver-view" role="group" aria-label="View">
      <button aria-pressed={view === 'list'} aria-label="List" onClick={() => onChange('list')}>
        <List size={16} aria-hidden="true" />
      </button>
      <button aria-pressed={view === 'cards'} aria-label="Cards" onClick={() => onChange('cards')}>
        <LayoutGrid size={16} aria-hidden="true" />
      </button>
    </div>
  );
}
