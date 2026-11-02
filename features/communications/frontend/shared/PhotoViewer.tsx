import {
  createContext,
  useContext,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { Download, X } from 'lucide-react';
import { download } from '../../../../core/shell/frontend/runtime/transfer.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox, useFocusTrap } from '../../../../core/shell/frontend/ui/index.js';
import './photo-viewer.css';

/** A photo of a message, to show over the page. */
type Photo = {
  /** Its file name, which names it to a screen reader. */
  name: string;
  /** The photo, as the page already shows it. */
  src: string;
  /** Where it's read from, to save it. */
  url: string;
};

const Viewing = createContext<((photo: Photo) => void) | null>(null);

/**
 * Where a thread's photos open: above its messages, so one open stays open when older messages
 * coming regroup the one it opened from. An image already shown keeps showing once the message's
 * own copy of it is let go, so the viewer shows that copy, not one fetched again.
 */
export function PhotoViewing({ children }: { children: ReactNode }) {
  const [shown, setShown] = useState<Photo>();
  return (
    <Viewing.Provider value={setShown}>
      {children}
      {shown && <PhotoViewer {...shown} onClose={() => setShown(undefined)} />}
    </Viewing.Provider>
  );
}

/** Opens a photo over the page, from within `PhotoViewing`. */
export function useViewPhoto() {
  const view = useContext(Viewing);
  if (!view) throw new Error('A photo opens within PhotoViewing.');
  return view;
}

/**
 * A photo over the page, as large as the screen allows it whole, with a button to save it. The
 * ×, Escape or a press outside the photo closes it. It sits on the page's body, apart from the
 * message it opened from, which may be dimmed.
 */
function PhotoViewer({ name, src, url, onClose }: Photo & { onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useFocusTrap(ref, { onEscape: onClose });
  // Inline: the page's own banner is under the viewer.
  const save = useAction(() => download(url), { inline: true });
  // Its width over its height, once it loads: the photo fills the screen at that shape.
  const [shape, setShape] = useState<number>();
  return createPortal(
    <div
      ref={ref}
      className="photo-viewer"
      role="dialog"
      aria-modal="true"
      aria-label={name}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <img
        src={src}
        alt={name}
        style={shape ? ({ '--shape': shape } as CSSProperties) : undefined}
        onLoad={(event) =>
          setShape(event.currentTarget.naturalWidth / event.currentTarget.naturalHeight)
        }
      />
      <ErrorBox message={save.error} />
      <div className="photo-viewer-actions">
        <button
          className="icon-button"
          aria-label="Download"
          disabled={save.busy}
          onClick={() => void save.run()}
        >
          <Download size={20} />
        </button>
        <button className="icon-button" aria-label="Close" onClick={onClose}>
          <X size={20} />
        </button>
      </div>
    </div>,
    document.body,
  );
}
