import { useState } from 'react';
import { ChevronLeft, PenLine, Pencil, Printer, Trash2 } from 'lucide-react';
import { ConfirmDialog, DataState } from '../../../../core/shell/frontend/ui/index.js';
import { usePicture } from '../../../../core/shell/frontend/runtime/usePicture.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { ConductOverview, ConductSaved, ConductWriteup } from '../../api/index.js';
import { copyUrl, deleteWriteup, useWriteup } from '../../api/client.js';
import { Notice } from '../notice/Notice.js';
import { usePrintNotice } from '../notice/usePrintNotice.js';
import { SignedState, categoryName, personOf } from '../parts.js';
import { SignDialog } from './SignDialog.js';

/** The photo of a writeup's signed paper. */
function SignedCopy({ writeup }: { writeup: ConductWriteup }) {
  const [box, src, lost] = usePicture<HTMLDivElement>(
    `${copyUrl(writeup.id)}?v=${writeup.revision}`,
  );
  return (
    <div ref={box} className="conduct-copy">
      {src && <img src={src} alt="Signed copy" onError={lost} />}
    </div>
  );
}

/** One writeup as it prints: printed, changed or deleted until it's signed, then only printed. */
export function WriteupView({
  overview,
  id,
  back,
  edit,
  take,
}: {
  overview: ConductOverview;
  id: string;
  back: () => void;
  edit: () => void;
  take: (overview: ConductOverview) => void;
}) {
  const row = overview.writeups.find((w) => w.id === id);
  // Read again whenever the list says it changed.
  const read = useWriteup(id, row ? `${row.revision}-${row.hasCopy}` : undefined);
  const { print, sheet } = usePrintNotice();
  const [signing, setSigning] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const remove = useAction(
    async (revision: number) => {
      take(await deleteWriteup(id, revision));
      back();
    },
    { success: 'Writeup deleted' },
  );
  return (
    <>
      <button type="button" className="conduct-back" onClick={back}>
        <ChevronLeft size={15} />
        Conduct
      </button>
      <DataState data={read.data} error={read.error} retry={read.refresh}>
        {(writeup) => {
          const person = personOf(overview, writeup.person);
          return (
            <>
              <div className="page-heading">
                <div>
                  <h1>{person.name}</h1>
                  <p className="conduct-subtitle">
                    {categoryName(overview, writeup.categoryId)} <SignedState writeup={writeup} />
                  </p>
                </div>
                <div className="heading-actions">
                  {!writeup.signedOn && (
                    <>
                      <button type="button" className="danger" onClick={() => setDeleting(true)}>
                        <Trash2 size={16} />
                        Delete
                      </button>
                      <button type="button" onClick={edit}>
                        <Pencil size={16} />
                        Edit
                      </button>
                    </>
                  )}
                  <button type="button" onClick={() => print(writeup)}>
                    <Printer size={16} />
                    Print
                  </button>
                  {!writeup.signedOn && (
                    <button type="button" className="primary" onClick={() => setSigning(true)}>
                      <PenLine size={16} />
                      Mark signed
                    </button>
                  )}
                </div>
              </div>
              <div className="conduct-paper">
                <Notice content={writeup} />
              </div>
              {writeup.hasCopy && (
                <section className="conduct-copy-section">
                  <h2 className="conduct-section-title">Signed copy</h2>
                  <SignedCopy writeup={writeup} />
                </section>
              )}
              {signing && (
                <SignDialog
                  overview={overview}
                  writeup={writeup}
                  close={() => setSigning(false)}
                  signed={(saved: ConductSaved) => {
                    take(saved.overview);
                    setSigning(false);
                  }}
                />
              )}
              {deleting && (
                <ConfirmDialog
                  title="Delete writeup"
                  confirm="Delete"
                  tone="danger"
                  busy={remove.busy}
                  onCancel={() => setDeleting(false)}
                  onConfirm={() => void remove.run(writeup.revision)}
                >
                  This writeup for {person.name} will be deleted.
                </ConfirmDialog>
              )}
            </>
          );
        }}
      </DataState>
      {sheet}
    </>
  );
}
