import { useState } from 'react';
import { Check } from 'lucide-react';
import { DateField, ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { ConductOverview, ConductSaved, ConductWriteupRow } from '../../api/index.js';
import { keepCopy, signWriteup } from '../../api/client.js';
import { Person, categoryName, personOf } from '../parts.js';
import { finalNote, printedDay, tierShort } from '../tiers.js';
import { randomId } from '../../../../core/shell/frontend/lib/random-id.js';
import { PhotoPicker, type PickedPhoto } from './PhotoPicker.js';
import { SignedCopy } from './SignedCopy.js';
import { usePhotoSave } from './usePhotoSave.js';

/** Marks a writeup signed on the day its paper was, with a photo of the paper signed. */
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
  const { phase, cancel, save } = usePhotoSave(close);
  const [attempted, setAttempted] = useState(false);
  // The photo this dialog kept, and the revision keeping it moved the writeup to.
  const [kept, setKept] = useState<{ photo: PickedPhoto; revision: number }>();
  const sign = useAction(
    () =>
      save(photo === kept?.photo ? undefined : photo, async (prepared) => {
        setAttempted(true);
        let revision = kept?.revision ?? notice.revision;
        if (prepared) {
          revision = (await keepCopy(notice.id, revision, prepared.bytes, prepared.request)).writeup
            .revision;
          setKept({ photo: prepared.photo, revision });
        }
        signed(await signWriteup(notice.id, day, revision, request));
      }),
    { inline: true, success: 'Marked signed' },
  );
  const note = notice.tier === 'final' ? finalNote(notice.finalKind, notice.suspension) : '';
  return (
    <Modal title="Mark signed" dismissible={phase !== 'saving'} onClose={cancel}>
      <div className="conduct-dialog conduct-sign">
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
            disabled={sign.busy || (!photo && !notice.hasCopy)}
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
