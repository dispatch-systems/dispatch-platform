import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { fitNotice, Notice, type NoticeContent } from './Notice.js';

/** Fonts and real sheet geometry are checked before opening the native print dialog. */
export function usePrintNotice() {
  const [printing, setPrinting] = useState<{ content: NoticeContent; after?: () => void }>();
  const [error, setError] = useState('');
  const paper = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!printing) return;
    let active = true;
    let frame: number | undefined;
    const done = () => {
      if (active) {
        setPrinting(undefined);
        printing.after?.();
      }
    };
    window.addEventListener('afterprint', done, { once: true });
    void document.fonts.ready.then(() => {
      if (!active) return;
      frame = requestAnimationFrame(() => {
        if (!active || !paper.current) return;
        fitNotice(paper.current);
        if (paper.current.querySelector('[data-unreadable],[data-overflow]')) {
          setError('Shorten the notice sections before printing.');
          done();
          return;
        }
        window.print();
      });
    });
    return () => {
      active = false;
      if (frame !== undefined) cancelAnimationFrame(frame);
      window.removeEventListener('afterprint', done);
    };
  }, [printing]);
  return {
    error,
    busy: Boolean(printing),
    print: (content: NoticeContent, after?: () => void) => {
      setError('');
      setPrinting({ content, after });
    },
    sheet: printing
      ? createPortal(
          <div ref={paper} className="conduct-print">
            <Notice content={printing.content} />
          </div>,
          document.body,
        )
      : null,
  };
}
