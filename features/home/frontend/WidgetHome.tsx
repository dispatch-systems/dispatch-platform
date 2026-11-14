import { Suspense, lazy, useMemo, useState } from 'react';
import { Pencil, Plus } from 'lucide-react';
import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import { useFillsWindow } from '../../../core/shell/frontend/runtime/fills.js';
import { usePreference } from '../../../core/shell/frontend/runtime/preferences.js';
import type { Widget } from '../../../core/shell/frontend/runtime/slots.js';
import { Header } from '../../../core/shell/frontend/ui/index.js';
import { shown, type Arrange } from './layout.js';
import { WidgetGrid } from './WidgetGrid.js';

// Loaded as the member asks to arrange the page, with what dragging and resizing need.
const loadEditor = () => import('./HomeEditor.js');
const HomeEditor = lazy(() => loadEditor().then((module) => ({ default: module.HomeEditor })));
const preload = () => void loadEditor().catch(() => undefined);

/**
 * The member's widgets where they placed them, none until they add one, and the page arranged in
 * place.
 */
export function WidgetHome({
  session,
  view,
  available,
}: {
  session: SessionView;
  view: DspView;
  available: readonly Widget[];
}) {
  // The grid fills the window below the page's header, which never scrolls.
  useFillsWindow();
  const saved = usePreference(view, 'home.layout');
  const placed = useMemo(
    () => shown(saved.value?.widgets ?? [], available),
    [saved.value, available],
  );
  // Whether the page is being arranged, and whether with the list of widgets to add open.
  const [editing, setEditing] = useState<{ adding: boolean }>();
  const arrange: Arrange = (update) =>
    saved.change((now) => ({ widgets: update(now?.widgets ?? []) }));
  const grid = <WidgetGrid placed={placed} view={view} />;
  return (
    <section className="home-page">
      {editing ? (
        <Suspense fallback={grid}>
          <HomeEditor
            view={view}
            available={available}
            placed={placed}
            adding={editing.adding}
            arrange={arrange}
            reset={() => saved.change(() => undefined)}
            done={() => setEditing(undefined)}
          />
        </Suspense>
      ) : (
        <>
          <Header title="Home">
            {!session.readOnly && (
              <button
                onPointerEnter={preload}
                onFocus={preload}
                onClick={() => setEditing({ adding: !placed.length })}
              >
                {placed.length ? <Pencil size={16} /> : <Plus size={16} />}
                {placed.length ? 'Edit' : 'Add widget'}
              </button>
            )}
          </Header>
          {grid}
        </>
      )}
    </section>
  );
}
