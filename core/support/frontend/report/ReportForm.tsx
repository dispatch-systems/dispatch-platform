import { useEffect, useRef, useState, type FormEvent } from 'react';
import { Bug, Lightbulb, Paperclip, Send, X } from 'lucide-react';
import { createTicket, uploadScreenshot } from '../../api/client.js';
import type { SupportDetail } from '../../api/index.js';
import { ErrorBox } from '../../../shell/frontend/ui/index.js';
import { randomId } from '../../../shell/frontend/lib/random-id.js';

type Screenshot = { id: string; file: File; preview: string; attachmentId?: string };
export function ReportForm({ onSubmitted }: { onSubmitted: (detail: SupportDetail) => void }) {
  const [kind, setKind] = useState('bug');
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [screenshots, setScreenshots] = useState<Screenshot[]>([]);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState('');
  const [error, setError] = useState('');
  const requestId = useRef(randomId());
  const previews = useRef(new Set<string>());
  const fileInput = useRef<HTMLInputElement>(null);
  useEffect(
    () => () => {
      for (const url of previews.current) URL.revokeObjectURL(url);
    },
    [],
  );
  const addFiles = (files: File[]) => {
    const accepted = files.filter((file) =>
      ['image/png', 'image/jpeg', 'image/webp'].includes(file.type),
    );
    if (accepted.length !== files.length) setError('Choose a PNG, JPEG or WebP screenshot.');
    setScreenshots((current) => [
      ...current,
      ...accepted.map((file) => {
        const preview = URL.createObjectURL(file);
        previews.current.add(preview);
        return { id: randomId(), file, preview };
      }),
    ]);
  };
  const removeFile = (screenshot: Screenshot) => {
    URL.revokeObjectURL(screenshot.preview);
    previews.current.delete(screenshot.preview);
    setScreenshots((current) => current.filter((file) => file.id !== screenshot.id));
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    setError('');
    try {
      const attachmentIds: string[] = [];
      for (const [index, screenshot] of screenshots.entries()) {
        setProgress(`Uploading screenshot ${index + 1} of ${screenshots.length}…`);
        const id = screenshot.attachmentId ?? (await uploadScreenshot(screenshot.file)).id;
        attachmentIds.push(id);
        setScreenshots((current) =>
          current.map((file) => (file.id === screenshot.id ? { ...file, attachmentId: id } : file)),
        );
      }
      setProgress('Sending your report…');
      const detail = await createTicket({
        requestId: requestId.current,
        kind,
        title: title.trim(),
        description: description.trim(),
        attachmentIds,
      });
      setTitle('');
      setDescription('');
      for (const screenshot of screenshots) URL.revokeObjectURL(screenshot.preview);
      previews.current.clear();
      setScreenshots([]);
      requestId.current = randomId();
      onSubmitted(detail);
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : 'Your report could not be sent. Try again.',
      );
    } finally {
      setBusy(false);
      setProgress('');
    }
  };
  return (
    <form
      className="support-report"
      onSubmit={submit}
      onPaste={(event) => {
        const files = Array.from(event.clipboardData.items)
          .filter((item) => item.kind === 'file' && item.type.startsWith('image/'))
          .map((item) => item.getAsFile())
          .filter((file): file is File => file !== null);
        if (files.length && !busy) {
          event.preventDefault();
          addFiles(files);
        }
      }}
    >
      <div className="support-intro">
        <h3>What can we help with?</h3>
        <p className="muted">Tell us what happened or what would make Dispatch better.</p>
      </div>
      <fieldset className="support-kind" disabled={busy}>
        <legend className="support-sr-only">Report type</legend>
        <label className={kind === 'bug' ? 'selected' : ''}>
          <input
            type="radio"
            name="kind"
            value="bug"
            checked={kind === 'bug'}
            onChange={() => setKind('bug')}
          />
          <Bug size={18} />
          <span>Bug report</span>
        </label>
        <label className={kind === 'feature' ? 'selected' : ''}>
          <input
            type="radio"
            name="kind"
            value="feature"
            checked={kind === 'feature'}
            onChange={() => setKind('feature')}
          />
          <Lightbulb size={18} />
          <span>Feature request</span>
        </label>
      </fieldset>
      <label>
        Title
        <input
          required
          maxLength={180}
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          disabled={busy}
          placeholder={
            kind === 'bug' ? 'A short summary of the issue' : 'What would you like to add?'
          }
        />
      </label>
      <label>
        Description
        <textarea
          required
          maxLength={40000}
          rows={5}
          value={description}
          onChange={(event) => setDescription(event.target.value)}
          disabled={busy}
          placeholder={
            kind === 'bug'
              ? 'What did you do? What happened, and what did you expect?'
              : 'Describe the feature and how it would help you.'
          }
        />
      </label>
      <div className="support-upload">
        <button type="button" onClick={() => fileInput.current?.click()} disabled={busy}>
          <Paperclip size={16} />
          Add screenshots
        </button>
        <span className="muted">Or paste a screenshot here.</span>
        <input
          ref={fileInput}
          className="support-sr-only"
          type="file"
          aria-label="Upload screenshots"
          accept="image/png,image/jpeg,image/webp"
          multiple
          disabled={busy}
          onChange={(event) => {
            addFiles(Array.from(event.target.files ?? []));
            event.target.value = '';
          }}
        />
      </div>
      {screenshots.length > 0 && (
        <ul className="support-screenshot-list">
          {screenshots.map((screenshot) => (
            <li key={screenshot.id}>
              <img src={screenshot.preview} alt={screenshot.file.name} />
              <span>{screenshot.file.name}</span>
              <button
                type="button"
                className="icon-button"
                aria-label={`Remove ${screenshot.file.name}`}
                onClick={() => removeFile(screenshot)}
                disabled={busy}
              >
                <X size={14} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <ErrorBox message={error} />
      <p className="support-report-note muted">
        An agent investigates first. The platform owner reviews the proposal before any changes
        begin.
      </p>
      <div className="support-report-submit">
        <span role="status">{progress}</span>
        <button className="primary" disabled={busy || !title.trim() || !description.trim()}>
          <Send size={16} />
          {busy ? 'Sending…' : 'Send report'}
        </button>
      </div>
    </form>
  );
}
