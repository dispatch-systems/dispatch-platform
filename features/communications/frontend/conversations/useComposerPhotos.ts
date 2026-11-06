import { useEffect, useRef, useState } from 'react';
import type { CommunicationsUpload, ComposeLimits } from '../../api/index.js';
import type { Attachments } from '../../api/client.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import { randomId } from '../../../../core/shell/frontend/lib/random-id.js';
import { size } from '../shared/format.js';

/** A photo added: shown at once, uploaded as it's added, and sent by what that answered. */
type Added = {
  key: string;
  file: File;
  shown: string;
  uploaded?: CommunicationsUpload;
  controller: AbortController;
};

/** The exact photos owned by one send; later additions belong to the next draft. */
type Sending = { keys: ReadonlySet<string>; photos: readonly Added[] };

/** Composer-local uploads, previews and send snapshots, kept out of text-only drafts. */
export function useComposerPhotos(limits: ComposeLimits | undefined, attachments: Attachments) {
  const { fail } = useFeedback();
  const [photos, setPhotos] = useState<Added[]>([]);
  // The photos of a send under way, which are no part of what's written meanwhile.
  const going = useRef<ReadonlySet<string>>(new Set());
  // Photos still uploading stop, and their previews go, as the composer does.
  const added = useRef(photos);
  useEffect(() => {
    added.current = photos;
  }, [photos]);
  useEffect(
    () => () => {
      for (const photo of added.current) {
        photo.controller.abort();
        URL.revokeObjectURL(photo.shown);
      }
    },
    [],
  );
  const remove = (key: string) => {
    const photo = added.current.find((each) => each.key === key);
    photo?.controller.abort();
    if (photo) URL.revokeObjectURL(photo.shown);
    setPhotos((now) => now.filter((each) => each.key !== key));
  };
  const add = (files: FileList | null, starts: () => void) => {
    const chosen = [...(files ?? [])];
    if (!limits) return;
    if (chosen.some((file) => !limits.kinds.includes(file.type)))
      return fail(`Photos must be ${kindsText(limits.kinds)}.`);
    if (chosen.some((file) => file.size > limits.largest))
      return fail(`Photos must be ${bytesText(limits.largest)} or smaller.`);
    if (photos.length + chosen.length > limits.photos)
      return fail(`Add up to ${limits.photos} photos.`);
    if (chosen.length) starts();
    const adding = chosen.map((file) => ({
      key: randomId(),
      file,
      shown: URL.createObjectURL(file),
      controller: new AbortController(),
    }));
    setPhotos((now) => [...now, ...adding]);
    for (const photo of adding)
      attachments.add(photo.file, photo.controller.signal).then(
        (uploaded) =>
          setPhotos((now) =>
            now.map((each) => (each.key === photo.key ? { ...each, uploaded } : each)),
          ),
        (cause) => {
          if (photo.controller.signal.aborted) return;
          remove(photo.key);
          fail(messageOf(cause));
        },
      );
  };
  const capture = (): Sending => {
    const keys = new Set(photos.map((photo) => photo.key));
    going.current = keys;
    return { keys, photos };
  };
  const finish = () => {
    going.current = new Set();
  };
  const settle = (sent: Sending) => {
    for (const photo of sent.photos) URL.revokeObjectURL(photo.shown);
    setPhotos((now) => now.filter((photo) => !sent.keys.has(photo.key)));
  };
  return {
    photos,
    add,
    remove,
    uploading: photos.some((photo) => !photo.uploaded),
    empty: () => photos.every((photo) => going.current.has(photo.key)),
    capture,
    finish,
    settle,
  };
}

/** The kinds of photo a message carries, as their names read: "PNG, JPEG or GIF". */
function kindsText(kinds: readonly string[]) {
  const names = kinds.map((kind) => kind.replace(/^image\//, '').toUpperCase());
  return names.length > 1 ? `${names.slice(0, -1).join(', ')} or ${names.at(-1)}` : names.join('');
}
/** The most bytes a photo may be, as it reads: "20 MB", or with a fraction, "2.5 MB". */
function bytesText(bytes: number) {
  const megabytes = bytes / 1024 ** 2;
  return Number.isInteger(megabytes) ? `${megabytes} MB` : size(bytes);
}
