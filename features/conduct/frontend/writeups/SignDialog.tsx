import { useState } from 'react';
import { Check, ImagePlus } from 'lucide-react';
import { DateField, ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { ConductOverview, ConductSaved, ConductWriteupRow } from '../../api/index.js';
import { keepCopy, signWriteup } from '../../api/client.js';
import { Person, categoryName, personOf } from '../parts.js';
import { finalNote, printedDay, tierShort } from '../tiers.js';
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
  const [photo, setPhoto] = useState<File>();
  const sign = useAction(
    async () => {
      // The photo first, of the notice as read: a signed writeup keeps the photo it has. Kept,
      // the writeup moves to the revision the sign then names.
      const kept = photo && (await keepCopy(notice.id, notice.revision, await shrunk(photo)));
      const revision = kept ? kept.writeup.revision : notice.revision;
      signed(await signWriteup(notice.id, day, revision));
    },
    { inline: true, success: 'Marked signed' },
  );
  const note = notice.tier === 'final' ? finalNote(notice.finalKind, notice.suspension) : '';
  return (
    <Modal title="Mark signed" onClose={close}>
      <div className="conduct-dialog">
        <div className="conduct-sign-summary">
          <Person person={personOf(overview, notice.person)} />
          <span className="muted">
            {categoryName(overview, notice.categoryId)} · {tierShort[notice.tier]}
            {note && ` ${note}`} · {printedDay(notice.writtenOn)}
          </span>
        </div>
        <div className="conduct-field">
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
        </div>
        <label className="conduct-photo">
          <span className="conduct-label">
            Signed copy <span className="muted">(optional)</span>
          </span>
          <span className="conduct-photo-pick">
            <ImagePlus size={20} />
            {photo ? photo.name : 'Take a photo or choose a file'}
          </span>
          <input
            type="file"
            accept="image/*"
            capture="environment"
            onChange={(event) => setPhoto(event.target.files?.[0])}
          />
        </label>
        <ErrorBox message={sign.error} />
        <div className="form-actions">
          <button type="button" onClick={close}>
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
