import { useRef, useState, type DragEvent as ReactDragEvent } from 'react';
import type { DocumentsItem } from '../../api/client.js';
import { carryCount, type Place } from '../Moving.js';
import { dropped, type Picked } from '../Uploads.js';
import { CARRIED } from './browsing.js';

/**
 * Dragging what the page shows onto a folder or a step of the path, which moves it, and dropping
 * files from the computer on them, which uploads them there. What's dragged is `chosen`, the
 * selection when the item is in it; a drop `move`s it, or `offer`s the files dropped.
 */
export function useDragging({
  chosen,
  move,
  offer,
}: {
  chosen: (item: DocumentsItem) => DocumentsItem[];
  move: (items: DocumentsItem[], to: Place) => void;
  offer: (files: Picked[], into?: string) => void;
}) {
  // Whether files from the computer are over the page, and the folder or step of the path under
  // the pointer as they or what the page carries are: '' for the top.
  const [filesOver, setFilesOver] = useState(false);
  const [over, setOver] = useState<string>();
  // What's being dragged, and the same once the browser has drawn it, to fade.
  const carried = useRef<DocumentsItem[]>(undefined);
  const [carrying, setCarrying] = useState<ReadonlySet<string>>();
  /** Starts dragging `item`, with the rest of the selection when it's in it. */
  const drag = (item: DocumentsItem) => (event: ReactDragEvent<HTMLElement>) => {
    const moving = chosen(item);
    carried.current = moving;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(CARRIED, moving.map((each) => each.id).join(' '));
    if (moving.length > 1) carryCount(event.dataTransfer, moving.length);
    else {
      const box = event.currentTarget.getBoundingClientRect();
      event.dataTransfer.setDragImage(
        event.currentTarget,
        event.clientX - box.left,
        event.clientY - box.top,
      );
    }
    setTimeout(() => {
      if (carried.current === moving) setCarrying(new Set(moving.map((each) => each.id)));
    });
  };
  const dragEnd = () => {
    carried.current = undefined;
    setCarrying(undefined);
    setOver(undefined);
  };
  /**
   * What lets `place` take a drop: what the page carries moves into it, and files from the
   * computer upload into it. Nothing goes into itself.
   */
  const takes = (place: Place) => {
    const key = place.id ?? '';
    const fits = (event: ReactDragEvent) =>
      carried.current
        ? !carried.current.some((item) => item.id === place.id)
        : event.dataTransfer.types.includes('Files');
    return {
      onDragOver: (event: ReactDragEvent) => {
        if (!fits(event)) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = carried.current ? 'move' : 'copy';
        setOver(key);
      },
      onDragLeave: (event: ReactDragEvent) => {
        if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
        setOver((now) => (now === key ? undefined : now));
      },
      onDrop: (event: ReactDragEvent) => {
        if (!fits(event)) return;
        event.preventDefault();
        event.stopPropagation();
        setFilesOver(false);
        const moving = carried.current;
        dragEnd();
        if (moving) move(moving, place);
        else void dropped(event.dataTransfer).then((files) => offer(files, place.id));
      },
    };
  };
  /** Whether what's dragged is the page's own, rather than files from the computer. */
  const carries = () => Boolean(carried.current);
  return { filesOver, setFilesOver, over, carrying, carries, drag, dragEnd, takes };
}
