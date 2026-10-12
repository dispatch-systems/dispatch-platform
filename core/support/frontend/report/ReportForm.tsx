import { useEffect, useId, useRef, useState, type FormEvent } from 'react';
import { Bug, ChevronLeft, ChevronRight, Lightbulb, Paperclip, X } from 'lucide-react';
import { createTicket, uploadScreenshot } from '../../api/client.js';
import type { SupportDetail } from '../../api/index.js';
import { ErrorBox } from '../../../shell/frontend/ui/index.js';
import { randomId } from '../../../shell/frontend/lib/random-id.js';

type Screenshot = { id: string; file: File; preview: string; attachmentId?: string };
export function ReportForm({
  onSubmitted,
  onStepChange,
}: {
  onSubmitted: (detail: SupportDetail) => void;
  onStepChange: () => void;
}) {
  const fieldId = useId();
  const [step, setStep] = useState<'type' | 'details'>('type');
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
  const titleInput = useRef<HTMLInputElement>(null);
  const kindChoices = useRef<HTMLFieldSetElement>(null);
  const changeStep = (next: 'type' | 'details') => {
    setStep(next);
    onStepChange();
  };
  useEffect(() => {
    if (step === 'details') titleInput.current?.focus();
    else kindChoices.current?.querySelector<HTMLInputElement>('input:checked')?.focus();
  }, [step]);
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
    if (step === 'type') {
      changeStep('details');
      return;
    }
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
      setStep('type');
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
        if (files.length && !busy && step === 'details') {
          event.preventDefault();
          addFiles(files);
        }
      }}
    >
      <div className="support-report-stepbar">
        {step === 'details' && (
          <button
            type="button"
            className="text-button support-report-back"
            disabled={busy}
            onClick={() => changeStep('type')}
          >
            <ChevronLeft size={14} />
            Back
          </button>
        )}
        <ol className="support-report-steps" aria-label="Report progress">
          <li aria-current={step === 'type' ? 'step' : undefined}>
            <span aria-hidden="true">1</span>Report type
          </li>
          <li aria-current={step === 'details' ? 'step' : undefined}>
            <span aria-hidden="true">2</span>Details
          </li>
        </ol>
      </div>
      <div className="support-intro">
        <h3>
          {step === 'type'
            ? 'What would you like to share?'
            : kind === 'bug'
              ? 'Tell us what happened.'
              : 'Describe your idea.'}
        </h3>
      </div>
      {step === 'type' ? (
        <fieldset className="support-kind" ref={kindChoices} disabled={busy}>
          <legend className="support-sr-only">Report type</legend>
          <label className={kind === 'bug' ? 'selected' : ''}>
            <input
              className="support-sr-only"
              type="radio"
              name="kind"
              value="bug"
              checked={kind === 'bug'}
              onChange={() => setKind('bug')}
            />
            <Bug size={28} />
            <span className="support-kind-copy">
              <strong>Report a bug</strong>
              <span>Something isn’t working as expected.</span>
            </span>
          </label>
          <label className={kind === 'feature' ? 'selected' : ''}>
            <input
              className="support-sr-only"
              type="radio"
              name="kind"
              value="feature"
              checked={kind === 'feature'}
              onChange={() => setKind('feature')}
            />
            <Lightbulb size={28} />
            <span className="support-kind-copy">
              <strong>Suggest a feature</strong>
              <span>Help us improve how Dispatch works.</span>
            </span>
          </label>
        </fieldset>
      ) : (
        <>
          <div className="support-report-field">
            <label htmlFor={`${fieldId}-title`}>Title</label>
            <input
              id={`${fieldId}-title`}
              ref={titleInput}
              type="text"
              required
              maxLength={180}
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              disabled={busy}
              placeholder={
                kind === 'bug' ? 'A short summary of the issue' : 'What would you like to add?'
              }
            />
          </div>
          <div className="support-report-field">
            <label htmlFor={`${fieldId}-description`}>Description</label>
            <textarea
              id={`${fieldId}-description`}
              required
              maxLength={40000}
              rows={7}
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              disabled={busy}
              placeholder={
                kind === 'bug'
                  ? 'What happened? What did you expect?'
                  : 'Describe the feature and how it would help.'
              }
            />
          </div>
          <div className="support-upload">
            <button type="button" onClick={() => fileInput.current?.click()} disabled={busy}>
              <Paperclip size={16} />
              Add screenshots
            </button>
            <span className="muted">or paste an image</span>
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
        </>
      )}
      <ErrorBox message={error} />
      <div className="support-report-submit">
        <span role="status">{progress}</span>
        {step === 'type' ? (
          <button
            key="continue"
            type="button"
            className="primary"
            onClick={() => changeStep('details')}
          >
            Continue <ChevronRight size={16} />
          </button>
        ) : (
          <button
            key="submit"
            type="submit"
            className="primary"
            disabled={busy || !title.trim() || !description.trim()}
          >
            {busy ? 'Sending…' : 'Send report'}
          </button>
        )}
      </div>
    </form>
  );
}
