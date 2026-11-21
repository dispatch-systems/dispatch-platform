import { useEffect, useRef, useState } from 'react';
import { useLeaveGuard } from '../../../../core/shell/frontend/runtime/leave.js';
import { randomId } from '../../../../core/shell/frontend/lib/random-id.js';
import type { PickedPhoto } from './PhotoPicker.js';
import { shrunk } from './photo.js';

/** A photo shrunk to send, with the request sending it: a retry of the same photo repeats it. */
export type PreparedPhoto = { photo: PickedPhoto; bytes: Blob; request: string };

/**
 * Saving a dialog's photo: preparing it stops at Cancel, or on leaving the page, until what
 * saves it starts, and then nothing closes the dialog until that ends. A photo is prepared
 * once, however often saving it is tried.
 */
export function usePhotoSave(close: () => void) {
  const preparation = useRef<AbortController | null>(null);
  const committing = useRef(false);
  const prepared = useRef<PreparedPhoto | null>(null);
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
  const [phase, setPhase] = useState<'idle' | 'preparing' | 'saving'>('idle');
  const cancel = () => {
    if (!committing.current) {
      preparation.current?.abort();
      close();
    }
  };
  /** Prepares `photo`, when there's one to send, then runs `commit` with it. */
  const save = async (
    photo: PickedPhoto | undefined,
    commit: (prepared?: PreparedPhoto) => Promise<void>,
  ) => {
    const stop = new AbortController();
    preparation.current = stop;
    setPhase('preparing');
    try {
      if (photo && prepared.current?.photo !== photo) {
        prepared.current = {
          photo,
          bytes: await shrunk(photo.file, stop.signal, photo.turns),
          request: randomId(),
        };
      }
      stop.signal.throwIfAborted();
      committing.current = true;
      setPhase('saving');
      await commit(photo ? (prepared.current ?? undefined) : undefined);
    } finally {
      committing.current = false;
      setPhase('idle');
    }
  };
  return { phase, cancel, save };
}
