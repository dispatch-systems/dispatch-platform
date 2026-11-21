import { useState } from 'react';
import { Check } from 'lucide-react';
import { ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { ConductWriteup } from '../../api/index.js';
import { keepCopy } from '../../api/client.js';
import { PhotoPicker, type PickedPhoto } from './PhotoPicker.js';
import { usePhotoSave } from './usePhotoSave.js';

/** Keeps a new photo of a signed writeup's paper in place of the one before, or its first. */
export function PhotoDialog({
  writeup,
  close,
  kept,
}: {
  writeup: ConductWriteup;
  close: () => void;
  kept: () => void;
}) {
  // The writeup as the dialog opened on it: a photo is kept only in place of the one it read.
  const [notice] = useState(writeup);
  const [photo, setPhoto] = useState<PickedPhoto>();
  const { phase, cancel, save } = usePhotoSave(close);
  const keep = useAction(
    (picked: PickedPhoto) =>
      save(picked, async (prepared) => {
        if (prepared) await keepCopy(notice.id, notice.revision, prepared.bytes, prepared.request);
        kept();
      }),
    { inline: true, success: 'Photo saved' },
  );
  return (
    <Modal
      title={notice.hasCopy ? 'Replace photo' : 'Add photo'}
      dismissible={phase !== 'saving'}
      onClose={cancel}
    >
      <div className="conduct-dialog conduct-sign">
        <PhotoPicker photo={photo} change={setPhoto} disabled={keep.busy} />
        <ErrorBox message={keep.error} />
        <div className="form-actions">
          <button type="button" disabled={phase === 'saving'} onClick={cancel}>
            Cancel
          </button>
          <button
            type="button"
            className="primary"
            disabled={keep.busy || !photo}
            onClick={() => photo && void keep.run(photo)}
          >
            <Check size={16} />
            {keep.busy ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>
    </Modal>
  );
}
