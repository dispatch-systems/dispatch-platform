import { useEffect, useState } from 'react';
import { picture } from '../../../../core/shell/frontend/runtime/transfer.js';
import { ErrorBox, Loading, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { copyUrl } from '../../api/client.js';

export function SignedCopy({ id, revision }: { id: string; revision: number }) {
  const [attempt, setAttempt] = useState(0);
  const [read, setRead] = useState<{ src?: string; error?: string }>({});
  const [large, setLarge] = useState(false);
  useEffect(() => {
    const stop = new AbortController();
    let src: string | undefined;
    setRead({});
    void picture(`${copyUrl(id)}?v=${revision}`, stop.signal).then(
      (blob) => {
        if (stop.signal.aborted) return;
        src = URL.createObjectURL(blob);
        setRead({ src });
      },
      () => {
        if (!stop.signal.aborted) setRead({ error: 'Couldn’t load the signed copy.' });
      },
    );
    return () => {
      stop.abort();
      if (src) URL.revokeObjectURL(src);
    };
  }, [id, revision, attempt]);
  const failed = () => setRead({ error: 'Couldn’t open the signed copy.' });
  return (
    <div className="conduct-copy">
      {!read.src && !read.error && <Loading />}
      <ErrorBox message={read.error ?? ''} />
      {read.error && (
        <button type="button" onClick={() => setAttempt((n) => n + 1)}>
          Retry
        </button>
      )}
      {read.src && (
        <>
          <button
            type="button"
            className="conduct-copy-open"
            aria-label="Enlarge signed copy"
            onClick={() => setLarge(true)}
          >
            <img src={read.src} alt="Signed copy" onError={failed} />
          </button>
          {large && (
            <Modal title="Signed copy" variant="browser" onClose={() => setLarge(false)}>
              <img
                className="conduct-copy-large"
                src={read.src}
                alt="Signed copy"
                onError={failed}
              />
            </Modal>
          )}
        </>
      )}
    </div>
  );
}
