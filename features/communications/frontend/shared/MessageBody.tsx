import { FileText } from 'lucide-react';
import type { Message, MessageFile } from '../../api/index.js';
import { download } from '../../../../core/shell/frontend/runtime/transfer.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { usePicture } from '../../../../core/shell/frontend/runtime/usePicture.js';
import { fileStatus, isPhoto, size } from './format.js';
import './message-body.css';

/** One message: its text, marked if edited in Slack, then its photos and files, read from where
 * the way it went keeps them (`address`). */
export function MessageBody({
  message,
  address,
}: {
  message: Pick<Message, 'text' | 'editedAt' | 'files'> & Partial<Pick<Message, 'status'>>;
  /** Where a photo or file of it is read from, by the way it went. */
  address: (id: string) => string;
}) {
  const photos = message.files.filter((file) => file.status === 'kept' && isPhoto(file.mime));
  const files = message.files.filter((file) => !photos.includes(file));
  return (
    <>
      {message.text && (
        <div className={message.status === 'failed' ? 'bubble failed' : 'bubble'}>
          {message.text}
          {message.editedAt !== null && <span className="edited">Edited</span>}
        </div>
      )}
      {photos.map((photo) => (
        <Photo key={photo.id} photo={photo} address={address} />
      ))}
      {files.map((file) => (
        <Attached key={file.id} file={file} address={address} />
      ))}
    </>
  );
}

/**
 * A photo, fetched as it nears view, as the page's own calls are, with the DSP's view; it opens
 * at full size in a tab of its own.
 */
function Photo({ photo, address }: { photo: MessageFile; address: (id: string) => string }) {
  const [box, src, lost] = usePicture<HTMLAnchorElement>(address(photo.id));
  return (
    <a ref={box} className="photo" href={src} target="_blank" rel="noreferrer">
      {src && <img src={src} alt={photo.name} onError={lost} />}
    </a>
  );
}

/** A file that isn't a photo, or one Dispatch doesn't keep: its name, and its size or why. */
function Attached({ file, address }: { file: MessageFile; address: (id: string) => string }) {
  const save = useAction(() => download(address(file.id)));
  const about = (
    <>
      <FileText size={18} aria-hidden="true" />
      <span className="file-name">{file.name}</span>
      <small>{file.status === 'kept' ? size(file.size) : fileStatus[file.status]}</small>
    </>
  );
  return file.status === 'kept' ? (
    <button className="file" disabled={save.busy} onClick={() => void save.run()}>
      {about}
    </button>
  ) : (
    <span className="file unkept">{about}</span>
  );
}
