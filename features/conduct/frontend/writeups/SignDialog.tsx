import { useEffect, useRef, useState } from 'react';
import { Check } from 'lucide-react';
import { DateField, ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { ConductOverview, ConductSaved, ConductWriteupRow } from '../../api/index.js';
import { keepCopy, signWriteup } from '../../api/client.js';
import { Person, categoryName, personOf } from '../parts.js';
import { finalNote, printedDay, tierShort } from '../tiers.js';
import { randomId } from '../../../../core/shell/frontend/lib/random-id.js';
import { useLeaveGuard } from '../../../../core/shell/frontend/runtime/leave.js';
import { PhotoPicker, type PickedPhoto } from './PhotoPicker.js';
import { SignedCopy } from './SignedCopy.js';
import { shrunk } from './photo.js';

/** Marks a writeup signed on the day its paper was, with a photo of the paper if there is one. */
export function SignDialog({
  overview,
  writeup,
  close,
  signed,
}: {
  overview: ConductOverview;
  writeup: ConductWriteupRow;
  close: () => void;
  signed: (saved: ConductSaved) => void;
}) {
  // The notice as the dialog opened on it: a later read of the writeup doesn't move it, so a
  // photo of its paper is never kept for, nor signed as, a changed one.
  const [notice] = useState(writeup);
  const [day, setDay] = useState(overview.today);
  const [photo, setPhoto] = useState<PickedPhoto>();
  const [request] = useState(randomId);
  const preparation = useRef<AbortController | null>(null);
  const committing = useRef(false);
  const prepared = useRef<{ photo: PickedPhoto; bytes: Blob; request: string } | null>(null);
  const guard = useRef<((then: () => void) => void) | null>(null);
  useEffect(() => {
    guard.current = (then) => {
      if (!committing.current) {
        preparation.current?.abort();
        then();
      }
    };
  });
  useLeaveGuard(guard);
  useEffect(() => () => preparation.current?.abort(), []);
  const cancel = () => {
    if (!committing.current) {
      preparation.current?.abort();
      close();
    }
  };
  const [attempted, setAttempted] = useState(false);
  const [phase, setPhase] = useState<'idle' | 'preparing' | 'saving'>('idle');
  // The photo this dialog kept, and the revision keeping it moved the writeup to.
  const [kept, setKept] = useState<{ photo: PickedPhoto; revision: number }>();
  const sign = useAction(
    async () => {
      const stop = new AbortController();
      preparation.current = stop;
      setPhase('preparing');
      try {
        let revision = kept?.revision ?? notice.revision;
        if (photo && photo !== kept?.photo && prepared.current?.photo !== photo) {
          prepared.current = {
            photo,
            bytes: await shrunk(photo.file, stop.signal, photo.turns),
            request: randomId(),
          };
        }
        stop.signal.throwIfAborted();
        setAttempted(true);
        committing.current = true;
        setPhase('saving');
        if (photo && photo !== kept?.photo && prepared.current) {
          revision = (
            await keepCopy(notice.id, revision, prepared.current.bytes, prepared.current.request)
          ).writeup.revision;
          setKept({ photo, revision });
        }
        signed(await signWriteup(notice.id, day, revision, request));
      } finally {
        committing.current = false;
        setPhase('idle');
      }
    },
    { inline: true, success: 'Marked signed' },
  );
  const note = notice.tier === 'final' ? finalNote(notice.finalKind, notice.suspension) : '';
  return (
    <Modal title="Mark signed" dismissible={phase !== 'saving'} onClose={cancel}>
      <div className="conduct-dialog">
        <div className="conduct-sign-summary">
          <Person person={personOf(overview, notice.person)} />
          <span className="muted">
            {categoryName(overview, notice.categoryId)} · {tierShort[notice.tier]}
            {note && ` ${note}`} · {printedDay(notice.writtenOn)}
          </span>
        </div>
        <fieldset className="conduct-field" disabled={sign.busy || attempted}>
          <span className="conduct-label" aria-hidden="true">
            Signed on
          </span>
          <DateField
            label="Signed on"
            value={day}
            min={notice.writtenOn}
            max={overview.today}
            today={overview.today}
            onChange={setDay}
          />
        </fieldset>
        <PhotoPicker photo={photo} change={setPhoto} disabled={sign.busy || attempted} />
        {!photo && notice.hasCopy && <SignedCopy id={notice.id} revision={notice.revision} />}
        <ErrorBox message={sign.error} />
        <div className="form-actions">
          <button type="button" disabled={phase === 'saving'} onClick={cancel}>
            Cancel
          </button>
          <button
            type="button"
            className="primary"
            disabled={sign.busy}
            onClick={() => void sign.run()}
          >
            <Check size={16} />
            {sign.busy ? 'Saving…' : 'Mark signed'}
          </button>
        </div>
      </div>
    </Modal>
  );
}
