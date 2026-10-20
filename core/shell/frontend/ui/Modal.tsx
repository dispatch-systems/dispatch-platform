import { useEffect, useId, useRef, type ReactNode } from 'react';
import { ArrowLeft, X } from 'lucide-react';
import { useFocusTrap } from './useFocusTrap.js';

export function Modal({
  title,
  children,
  onClose,
  variant = 'dialog',
  description,
  dismissible = true,
  initialFocus = 'input,button,select',
  onBack,
  above = false,
  page = false,
}: {
  title: ReactNode;
  description?: string;
  dismissible?: boolean;
  initialFocus?: string;
  children: ReactNode;
  onClose: () => void;
  variant?: 'dialog' | 'sheet' | 'browser';
  /** A dialog in steps: goes back a step, from an arrow before the title. */
  onBack?: () => void;
  /** Shown over any other dialog or sheet open, as a prompt the page itself raised is. */
  above?: boolean;
  /**
   * Over the page alone: the app's sidebar and bars stay in use, so focus isn't held in it, and
   * whoever opens it makes what it covers `inert`.
   */
  page?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const headingId = useId();
  const dismiss = () => {
    if (dismissible) onClose();
  };
  useFocusTrap(ref, { active: !page, initialFocus, onEscape: dismiss });
  // Over the page alone, it takes focus as it opens, and leaves it free to go.
  useEffect(() => {
    if (page) ref.current?.querySelector<HTMLElement>(initialFocus)?.focus();
  }, [page, initialFocus]);
  return (
    <div
      className={`modal-backdrop ${variant === 'sheet' ? 'sheet-backdrop' : ''} ${above ? 'modal-above' : ''} ${page ? 'page-modal' : ''}`}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) dismiss();
      }}
    >
      <div
        ref={ref}
        className={`modal ${variant === 'sheet' ? 'side-sheet' : variant === 'browser' ? 'browser-modal' : ''}`}
        role="dialog"
        aria-modal={page ? undefined : 'true'}
        aria-labelledby={headingId}
        onKeyDown={page ? (event) => event.key === 'Escape' && dismiss() : undefined}
      >
        <div className="modal-heading" data-slot={variant === 'sheet' ? 'sheet-header' : undefined}>
          {onBack ? (
            <div className="modal-title">
              <button className="icon-button" aria-label="Back" onClick={onBack}>
                <ArrowLeft size={18} />
              </button>
              <h2 id={headingId}>{title}</h2>
            </div>
          ) : (
            <h2 id={headingId}>{title}</h2>
          )}
          {description && <p className="muted">{description}</p>}
          {variant !== 'sheet' && dismissible && (
            <button className="icon-button" aria-label="Close dialog" onClick={onClose}>
              <X size={20} />
            </button>
          )}
        </div>
        {variant === 'sheet' ? <div className="panel-body">{children}</div> : children}
        {variant === 'sheet' && dismissible && (
          <button className="sheet-close" aria-label="Close dialog" onClick={onClose}>
            <X size={16} />
          </button>
        )}
      </div>
    </div>
  );
}
