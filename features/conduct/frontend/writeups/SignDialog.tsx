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
  const [day, setDay] = useState(overview.today);
  const [photo, setPhoto] = useState<File>();
  const sign = useAction(
    async () => {
      // The photo first: a writeup once signed keeps the photo it has.
      if (photo) await keepCopy(writeup.id, await shrunk(photo));
      signed(await signWriteup(writeup.id, day, writeup.revision));
    },
    { inline: true, success: 'Marked signed' },
  );
  const note = writeup.tier === 'final' ? finalNote(writeup.finalKind, writeup.suspension) : '';
  return (
    <Modal title="Mark signed" onClose={close}>
      <div className="conduct-dialog">
        <div className="conduct-sign-summary">
          <Person person={personOf(overview, writeup.person)} />
          <span className="muted">
            {categoryName(overview, writeup.categoryId)} · {tierShort[writeup.tier]}
            {note && ` ${note}`} · {printedDay(writeup.writtenOn)}
          </span>
        </div>
        <div className="conduct-field">
          <span className="conduct-label" aria-hidden="true">
            Signed on
          </span>
          <DateField
            label="Signed on"
            value={day}
            min={writeup.writtenOn}
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
