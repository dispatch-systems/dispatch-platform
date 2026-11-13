import { useState } from 'react';
import { ChevronLeft, PenLine, Pencil, Printer, Trash2 } from 'lucide-react';
import { ConfirmDialog, DataState, ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { ConductOverview, ConductWriteup } from '../../api/index.js';
import { deleteWriteup, readWriteup, useWriteup } from '../../api/client.js';
import { NoticePaper } from '../notice/NoticePaper.js';
import { SignedCopy } from './SignedCopy.js';
import { usePrintNotice } from '../notice/usePrintNotice.js';
import { SignedState, categoryName, personOf } from '../parts.js';
import { SignDialog } from './SignDialog.js';

/**
 * One writeup as it prints: printed, changed or deleted until it's signed, then only printed,
 * or deleted by someone who manages Conduct (`manages`).
 */
export function WriteupView({
  overview,
  id,
  manages,
  back,
  edit,
  take,
}: {
  overview: ConductOverview;
  id: string;
  manages: boolean;
  back: () => void;
  edit: () => void;
  take: (overview?: ConductOverview) => void;
}) {
  const read = useWriteup(id);
  const { print, sheet, error: printError } = usePrintNotice();
  const [signing, setSigning] = useState<ConductWriteup>();
  const beginSign = useAction(async () => {
    const fresh = await readWriteup(id);
    if (!fresh.signedOn) setSigning(fresh);
  });
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
      <ErrorBox message={printError} />
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
                  {(!writeup.signedOn || manages) && (
                    <button type="button" className="danger" onClick={() => setDeleting(true)}>
                      <Trash2 size={16} />
                      Delete
                    </button>
                  )}
                  {!writeup.signedOn && (
                    <button type="button" onClick={edit}>
                      <Pencil size={16} />
                      Edit
                    </button>
                  )}
                  <button type="button" onClick={() => print(writeup)}>
                    <Printer size={16} />
                    Print
                  </button>
                  {!writeup.signedOn && (
                    <button
                      type="button"
                      className="primary"
                      disabled={beginSign.busy}
                      onClick={() => void beginSign.run()}
                    >
                      <PenLine size={16} />
                      Mark signed
                    </button>
                  )}
                </div>
              </div>
              <NoticePaper content={writeup} />
              {writeup.hasCopy && (
                <section className="conduct-copy-section">
                  <h2 className="conduct-section-title">Signed copy</h2>
                  <SignedCopy id={writeup.id} revision={writeup.revision} />
                </section>
              )}
              {signing && (
                <SignDialog
                  overview={overview}
                  writeup={signing}
                  close={() => setSigning(undefined)}
                  signed={() => {
                    take();
                    setSigning(undefined);
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
