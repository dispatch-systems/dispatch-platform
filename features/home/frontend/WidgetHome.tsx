import { Suspense, lazy, useMemo, useState } from 'react';
import { Pencil, Plus } from 'lucide-react';
import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import { usePreference } from '../../../core/shell/frontend/runtime/preferences.js';
import type { Widget } from '../../../core/shell/frontend/runtime/slots.js';
import { Header } from '../../../core/shell/frontend/ui/index.js';
import type { PlacedWidget } from '../api/index.js';
import { shown, starting } from './layout.js';
import { WidgetGrid } from './WidgetGrid.js';

// Loaded as the member asks to arrange the page, with what dragging and resizing need.
const loadEditor = () => import('./HomeEditor.js');
const HomeEditor = lazy(() => loadEditor().then((module) => ({ default: module.HomeEditor })));
const preload = () => void loadEditor().catch(() => undefined);

/** A change to the widgets a member placed, made on those kept when it's saved. */
export type Arrange = (update: (placed: PlacedWidget[]) => PlacedWidget[]) => void;

/**
 * The member's widgets where they placed them, or where the features start them until they
 * arrange their own, and the page arranged in place.
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
  const saved = usePreference(view, 'home.layout');
  const layout = useMemo(
    () => saved.value?.widgets ?? starting(available),
    [saved.value, available],
  );
  const placed = useMemo(() => shown(layout, available), [layout, available]);
  // Whether the page is being arranged, and whether with the list of widgets to add open.
  const [editing, setEditing] = useState<{ adding: boolean }>();
  const arrange: Arrange = (update) =>
    saved.change((now) => ({ widgets: update(now?.widgets ?? starting(available)) }));
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
