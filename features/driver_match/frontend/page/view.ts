import { LayoutGrid, List } from 'lucide-react';
import { useState } from 'react';
import type { ViewOption } from '../../../../core/shell/frontend/ui/index.js';

/** How the page shows people: a row each, or a card each. */
export type View = 'list' | 'cards';

/** The ways it can, across from the page's name. */
export const VIEWS: readonly ViewOption<View>[] = [
  { value: 'list', label: 'List', icon: List },
  { value: 'cards', label: 'Cards', icon: LayoutGrid },
];

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
