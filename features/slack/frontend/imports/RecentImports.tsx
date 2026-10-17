import { AlertCircle, Check, Clock, File, Film, Image, Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { time } from '../../../../core/shell/frontend/lib/format.js';
import type { SlackImportedFile } from '../../api/client.js';

// The files noticed in linked channels latest, with who posted each where, and how saving it
// went: saved, with where it opens, or why not yet, or not at all.

export function RecentImports({
  files,
  timeZone,
}: {
  files: readonly SlackImportedFile[];
  timeZone: string;
}) {
  return (
    <div className="slack-channel-group">
      <h3>Recent files</h3>
      <ul className="slack-kept-list" aria-label="Recent files">
        {files.map((file) => (
          <li key={file.fileId}>
            <span className="slack-kept-kind">{kind(file.mime)}</span>
            <div className="slack-kept-what">
              <b>{file.name ?? 'File'}</b>
              <span>
                {[file.postedBy, file.channelName && `#${file.channelName}`]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </div>
            <span className="slack-kept-when">{time(file.noticedAt, timeZone)}</span>
            {status(file)}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** A file's mark, by its type. */
function kind(mime: string | null) {
  if (mime?.startsWith('image/')) return <Image size={16} aria-hidden="true" />;
  if (mime?.startsWith('video/')) return <Film size={16} aria-hidden="true" />;
  return <File size={16} aria-hidden="true" />;
}

/** How saving the file went, in words, with where it opens once saved. */
function status(file: SlackImportedFile): ReactNode {
  switch (file.status) {
    case 'saved':
      return (
        <>
          <span className="slack-kept-status saved">
            <Check size={14} aria-hidden="true" />
            Saved
          </span>
          {file.link && (
            <a className="slack-kept-open" href={file.link} target="_blank" rel="noreferrer">
              Open
            </a>
          )}
        </>
      );
    case 'waiting':
      return (
        <span className="slack-kept-status waiting">
          <Clock size={14} aria-hidden="true" />
          {file.retrying ? 'Retrying' : 'Saving'}
        </span>
      );
    case 'too_large':
      return (
        <span className="slack-kept-status left">
          <AlertCircle size={14} aria-hidden="true" />
          Over 100 MB
        </span>
      );
    case 'gone':
      return (
        <span className="slack-kept-status waiting">
          <Trash2 size={14} aria-hidden="true" />
          Deleted in Slack
        </span>
      );
    case 'failed':
      return (
        <span className="slack-kept-status left">
          <AlertCircle size={14} aria-hidden="true" />
          Couldn't save
        </span>
      );
  }
}
