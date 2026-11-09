import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { Notice, type NoticeContent } from './Notice.js';

/**
 * Prints a notice on its own: `print` puts it on a sheet only printing shows, beside the page,
 * opens the browser's print dialog, and once it closes calls `after`; `sheet` is that sheet,
 * drawn wherever the screen is.
 */
export function usePrintNotice() {
  const [printing, setPrinting] = useState<{ content: NoticeContent; after?: () => void }>();
  useEffect(() => {
    if (!printing) return;
    const done = () => {
      setPrinting(undefined);
      printing.after?.();
    };
    window.addEventListener('afterprint', done, { once: true });
    window.print();
    return () => window.removeEventListener('afterprint', done);
  }, [printing]);
  return {
    print: (content: NoticeContent, after?: () => void) => setPrinting({ content, after }),
    sheet: printing
      ? createPortal(
          <div className="conduct-print">
            <Notice content={printing.content} />
          </div>,
          document.body,
        )
      : null,
  };
}
