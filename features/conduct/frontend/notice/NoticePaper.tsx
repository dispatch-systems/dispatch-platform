import { useEffect, useRef, useState } from 'react';
import { Maximize2 } from 'lucide-react';
import { Modal } from '../../../../core/shell/frontend/ui/index.js';
import { Notice, type NoticeContent } from './Notice.js';

/** Scale against the actual available width, with an actual-size view for reading. */
export function NoticePaper({
  content,
  marked = false,
  preview = false,
}: {
  content: NoticeContent;
  marked?: boolean;
  preview?: boolean;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(0.4);
  const [large, setLarge] = useState(false);
  useEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setScale(Math.min(1, entry.contentRect.width / 816));
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return (
    <div className={preview ? 'conduct-preview' : 'conduct-paper'}>
      <div className="conduct-paper-actions">
        <button type="button" className="text-button" onClick={() => setLarge(true)}>
          <Maximize2 size={15} />
          Enlarge
        </button>
      </div>
      <div ref={box} className="conduct-paper-width">
        <div style={{ zoom: scale }}>
          <Notice content={content} marked={marked} />
        </div>
      </div>
      {large && (
        <Modal title="Corrective Action Notice" variant="browser" onClose={() => setLarge(false)}>
          <div className="conduct-paper-full">
            <Notice content={content} marked={marked} />
          </div>
        </Modal>
      )}
    </div>
  );
}
