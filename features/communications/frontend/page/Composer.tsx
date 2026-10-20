import { useEffect, useRef, useState } from 'react';
import { ArrowUp, LoaderCircle, Paperclip, X } from 'lucide-react';
import type { CommunicationsUpload, Conversation, Message } from '../../api/index.js';
import { addPhoto, sayTyping, sendMessage } from '../../api/client.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import { randomId } from '../../../../core/shell/frontend/lib/random-id.js';

// Writing in a conversation: up to 3,000 characters, and up to ten photos, each PNG, JPEG or GIF
// up to 20 MB, uploaded as they're added. Enter sends, Shift+Enter starts a new line. While
// someone writes, their teammates see they are, said again every few seconds.

const TEXT_MOST = 3000;
const PHOTOS_MOST = 10;
const PHOTO_BYTES = 20 * 1024 * 1024;
const PHOTO_KINDS = ['image/png', 'image/jpeg', 'image/gif'];
/** How often writing is said again: the server counts it for six seconds. */
const TYPING_MS = 3000;

/** A photo added: shown at once, uploaded as it's added, and sent by what that answered. */
type Added = {
  key: string;
  file: File;
  shown: string;
  uploaded?: CommunicationsUpload;
  controller: AbortController;
};

export function Composer({
  conversation,
  writable,
  sent,
}: {
  conversation: Conversation;
  /** Whether Communications works in its workspace now. */
  writable: boolean;
  sent: (message: Message) => void;
}) {
  const { fail } = useFeedback();
  const [text, setText] = useState('');
  const [photos, setPhotos] = useState<Added[]>([]);
  const picker = useRef<HTMLInputElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const said = useRef(0);
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
  // The field grows with what's written, up to a few lines.
  useEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = 'auto';
    element.style.height = `${element.scrollHeight}px`;
  }, [text]);

  const remove = (key: string) => {
    const photo = added.current.find((each) => each.key === key);
    photo?.controller.abort();
    if (photo) URL.revokeObjectURL(photo.shown);
    setPhotos((now) => now.filter((each) => each.key !== key));
  };
  const add = (files: FileList | null) => {
    const chosen = [...(files ?? [])];
    if (chosen.some((file) => !PHOTO_KINDS.includes(file.type)))
      return fail('Photos must be PNG, JPEG or GIF.');
    if (chosen.some((file) => file.size > PHOTO_BYTES))
      return fail('Photos must be 20 MB or smaller.');
    if (photos.length + chosen.length > PHOTOS_MOST) return fail('Add up to 10 photos.');
    const adding = chosen.map((file) => ({
      key: randomId(),
      file,
      shown: URL.createObjectURL(file),
      controller: new AbortController(),
    }));
    setPhotos((now) => [...now, ...adding]);
    for (const photo of adding)
      addPhoto(photo.file, undefined, photo.controller.signal).then(
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
  const send = useAction(async () => {
    const message = await sendMessage(
      conversation.id,
      text.trim(),
      photos.map((photo) => photo.uploaded!.id),
    );
    for (const photo of photos) URL.revokeObjectURL(photo.shown);
    setPhotos([]);
    setText('');
    said.current = 0;
    sent(message);
  });
  const uploading = photos.some((photo) => !photo.uploaded);
  const ready =
    writable && !send.busy && !uploading && (text.trim().length > 0 || photos.length > 0);
  const writing = (value: string) => {
    setText(value);
    if (value.trim() && Date.now() - said.current > TYPING_MS) {
      said.current = Date.now();
      void sayTyping(conversation.id).catch(() => undefined);
    }
  };
  const first = conversation.with.name.split(' ')[0] ?? conversation.with.name;
  return (
    <form
      className="composer"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) void send.run();
      }}
    >
      {photos.length > 0 && (
        <ul className="adding">
          {photos.map((photo) => (
            <li key={photo.key}>
              <img src={photo.shown} alt={photo.file.name} />
              {!photo.uploaded && <LoaderCircle className="spin" size={18} aria-hidden="true" />}
              <button
                type="button"
                aria-label={`Remove ${photo.file.name}`}
                onClick={() => remove(photo.key)}
              >
                <X size={12} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="box">
        <button
          type="button"
          className="round"
          aria-label="Attach photos"
          disabled={!writable || photos.length >= PHOTOS_MOST}
          onClick={() => picker.current?.click()}
        >
          <Paperclip size={18} />
        </button>
        <input
          ref={picker}
          type="file"
          accept={PHOTO_KINDS.join(',')}
          multiple
          hidden
          onChange={(event) => {
            add(event.currentTarget.files);
            event.currentTarget.value = '';
          }}
        />
        <textarea
          ref={field}
          rows={1}
          aria-label="Message"
          placeholder={`Message ${first}`}
          maxLength={TEXT_MOST}
          value={text}
          disabled={!writable}
          onChange={(event) => writing(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing) return;
            event.preventDefault();
            if (ready) void send.run();
          }}
        />
        <button type="submit" className="round send" aria-label="Send" disabled={!ready}>
          {send.busy ? <LoaderCircle className="spin" size={18} /> : <ArrowUp size={18} />}
        </button>
      </div>
    </form>
  );
}
