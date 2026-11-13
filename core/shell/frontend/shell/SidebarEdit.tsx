import { useEffect, useRef, type KeyboardEvent } from 'react';
import {
  DndContext,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
  type Modifier,
} from '@dnd-kit/core';
import {
  SortableContext,
  arrayMove,
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { Eye, EyeOff, GripVertical, type LucideIcon } from 'lucide-react';

/** What it shows of an item of the app's sidebar. */
type Item = { id: string; label: string; icon?: LucideIcon };

/** An item dragged moves up and down its list, never off to the side. */
const upAndDown: Modifier = ({ transform }) => ({ ...transform, x: 0 });

/** An item listed, moved by its handle, dragged or with the arrow keys, and hidden by its eye. */
function Listed({
  item,
  step,
  hide,
}: {
  item: Item;
  /** Moves it one place up or down. */
  step: (by: -1 | 1) => void;
  hide: () => void;
}) {
  const { setNodeRef, setActivatorNodeRef, listeners, transform, transition, isDragging } =
    useSortable({ id: item.id });
  const Icon = item.icon;
  const keyed = (event: KeyboardEvent) => {
    if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
    event.preventDefault();
    step(event.key === 'ArrowUp' ? -1 : 1);
  };
  return (
    <li
      ref={setNodeRef}
      className={isDragging ? 'nav-item sidebar-edit-item dragging' : 'nav-item sidebar-edit-item'}
      style={{ transform: CSS.Translate.toString(transform), transition }}
    >
      <button
        ref={setActivatorNodeRef}
        type="button"
        className="sidebar-edit-grip"
        aria-label={`Move ${item.label}`}
        onKeyDown={keyed}
        {...listeners}
      >
        <GripVertical aria-hidden="true" />
      </button>
      {Icon && <Icon aria-hidden="true" />}
      <span>{item.label}</span>
      <button type="button" aria-label={`Hide ${item.label}`} onClick={hide}>
        <Eye aria-hidden="true" />
      </button>
    </li>
  );
}

/**
 * The app's sidebar while its member arranges it, in place of its links: each item they list
 * with a handle to move it and an eye to hide it, the one kept last below them, and those
 * hidden under their own heading, each with the eye that lists it again where it was. Every
 * change is arranged at once (`arrange`, with every item in its order and those hidden).
 */
export function SidebarEdit({
  items,
  hidden,
  fixed,
  arrange,
  reset,
  done,
}: {
  /** Every item that moves, in the member's order, the hidden ones among them. */
  items: readonly Item[];
  hidden: readonly string[];
  /** The item kept last, which neither moves nor hides. */
  fixed?: Item;
  arrange: (ids: string[], hidden: string[]) => void;
  reset: () => void;
  done: () => void;
}) {
  const sensors = useSensors(useSensor(PointerSensor));
  const finish = useRef<HTMLButtonElement>(null);
  useEffect(() => finish.current?.focus({ preventScroll: true }), []);
  const ids = items.map((item) => item.id);
  const listed = items.filter((item) => !hidden.includes(item.id));
  const unlisted = items.filter((item) => hidden.includes(item.id));
  // The listed items take their new order in the places they held, the hidden keeping theirs.
  const move = (from: number, to: number) => {
    if (to < 0 || to >= listed.length) return;
    const moved = arrayMove(listed, from, to);
    let next = 0;
    arrange(
      items.map((item) => (hidden.includes(item.id) ? item.id : moved[next++]!.id)),
      [...hidden],
    );
  };
  const dropped = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) return;
    const at = (id: unknown) => listed.findIndex((item) => item.id === id);
    move(at(active.id), at(over.id));
  };
  const FixedIcon = fixed?.icon;
  return (
    <div className="nav-list app-pages sidebar-edit" role="group" aria-label="Edit sidebar">
      <div className="sidebar-edit-bar">
        <button type="button" onClick={reset}>
          Reset
        </button>
        <button ref={finish} type="button" className="primary" onClick={done}>
          Done
        </button>
      </div>
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        modifiers={[upAndDown]}
        onDragEnd={dropped}
      >
        <SortableContext
          items={listed.map((item) => item.id)}
          strategy={verticalListSortingStrategy}
        >
          <ul>
            {listed.map((item, at) => (
              <Listed
                key={item.id}
                item={item}
                step={(by) => move(at, at + by)}
                hide={() => arrange(ids, [...hidden, item.id])}
              />
            ))}
          </ul>
        </SortableContext>
      </DndContext>
      {fixed && (
        <div className="nav-item nav-apart sidebar-edit-item">
          {FixedIcon && <FixedIcon aria-hidden="true" />}
          <span>{fixed.label}</span>
        </div>
      )}
      {unlisted.length > 0 && (
        <>
          <h2>Hidden</h2>
          <ul>
            {unlisted.map(({ id, label, icon: Icon }) => (
              <li key={id} className="nav-item sidebar-edit-item unlisted">
                {Icon && <Icon aria-hidden="true" />}
                <span>{label}</span>
                <button
                  type="button"
                  aria-label={`Show ${label}`}
                  onClick={() =>
                    arrange(
                      ids,
                      hidden.filter((each) => each !== id),
                    )
                  }
                >
                  <EyeOff aria-hidden="true" />
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
