import { useRef } from 'react';
import { Download, X } from 'lucide-react';
import { download } from '../../../../core/shell/frontend/runtime/transfer.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { useFocusTrap } from '../../../../core/shell/frontend/ui/index.js';
import './photo-viewer.css';

/**
 * A photo of a message over the page, as large as the screen allows it whole, with a button to
 * save it. The ×, Escape or a press outside the photo closes it.
 */
export function PhotoViewer({
  name,
  src,
  url,
  onClose,
}: {
  /** The photo's file name, which names it to a screen reader. */
  name: string;
  /** The photo, as the page already shows it. */
  src: string;
  /** Where the photo is read from, to save it. */
  url: string;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useFocusTrap(ref, { onEscape: onClose });
  const save = useAction(() => download(url));
  return (
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
      <img src={src} alt={name} />
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
    </div>
  );
}
