import {
  createContext,
  useContext,
  useEffect,
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

/** A photo of a message, or a video, to show over the page. */
type Photo =
  | {
      /** Its file name, which names it to a screen reader, and a video's as it's saved. */
      name: string;
      /** The photo, as the page already shows it. */
      src: string;
      /** Where it's read from, to save it. */
      url: string;
      video?: false;
    }
  | {
      name: string;
      /** The video's bytes, the viewer's alone: saved as they are, let go of as it closes. */
      src: string;
      video: true;
    };

/** How long a video's bytes are kept once the viewer lets go of them, for a save still reading them. */
const SAVING_MS = 60_000;

const Viewing = createContext<((photo: Photo) => void) | null>(null);

/**
 * Where a thread's photos and videos open: above its messages, so one open stays open when older
 * messages coming regroup the one it opened from. An image already shown keeps showing once the
 * message's own copy of it is let go, so the viewer shows that copy, not one fetched again.
 */
export function PhotoViewing({ children }: { children: ReactNode }) {
  const [shown, setShown] = useState<Photo>();
  // A video's bytes are let go of once another opens, it closes, or the thread it's over goes: a
  // moment after, as a save of them started as it closed still reads them.
  const video = shown?.video ? shown.src : undefined;
  useEffect(
    () => () => void (video && setTimeout(() => URL.revokeObjectURL(video), SAVING_MS)),
    [video],
  );
  return (
    <Viewing.Provider value={setShown}>
      {children}
      {shown && <PhotoViewer {...shown} onClose={() => setShown(undefined)} />}
    </Viewing.Provider>
  );
}

/** Opens a photo or video over the page, from within `PhotoViewing`. */
export function useViewPhoto() {
  const view = useContext(Viewing);
  if (!view) throw new Error('A photo opens within PhotoViewing.');
  return view;
}

/**
 * A photo over the page, as large as the screen allows it whole, or a video playing, with a
 * button to save it. The ×, Escape or a press outside it closes it. It sits on the page's body,
 * apart from the message it opened from, which may be dimmed.
 */
function PhotoViewer({ onClose, ...shown }: Photo & { onClose: () => void }) {
  const { name, src, video } = shown;
  const ref = useRef<HTMLDivElement>(null);
  useFocusTrap(ref, { onEscape: onClose });
  // Inline: the page's own banner is under the viewer. A video saves the bytes it plays.
  const save = useAction(async () => (shown.video ? saved(src, name) : download(shown.url)), {
    inline: true,
  });
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
      {video ? (
        // Reached by Tab, for its controls; one the browser can't play after all is saved instead.
        <video
          src={src}
          aria-label={name}
          tabIndex={0}
          controls
          autoPlay
          playsInline
          onError={() => {
            saved(src, name);
            onClose();
          }}
          style={shape ? ({ '--shape': shape } as CSSProperties) : undefined}
          onLoadedMetadata={(event) =>
            setShape(event.currentTarget.videoWidth / event.currentTarget.videoHeight)
          }
        />
      ) : (
        <img
          src={src}
          alt={name}
          style={shape ? ({ '--shape': shape } as CSSProperties) : undefined}
          onLoad={(event) =>
            setShape(event.currentTarget.naturalWidth / event.currentTarget.naturalHeight)
          }
        />
      )}
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

/** Saves the bytes at `src`, the page's own address of them, as a file named `name`. */
function saved(src: string, name: string) {
  const link = document.createElement('a');
  link.href = src;
  link.download = name;
  link.click();
}
