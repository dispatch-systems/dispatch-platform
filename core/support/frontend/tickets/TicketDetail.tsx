import { useRef, useState, type FormEvent } from 'react';
import { ArrowLeft, CheckCircle2, ExternalLink, LockKeyhole, Send, XCircle } from 'lucide-react';
import { attachmentUrl, reopenTicket, sendMessage, useTicket } from '../../api/client.js';
import type { SupportDetail } from '../../api/index.js';
import { DataState, ErrorBox } from '../../../shell/frontend/ui/index.js';
import { TicketStatus, kindLabel, when } from '../format.js';
import { Proposal } from '../owner/Proposal.js';
import { IntakeReview } from '../owner/IntakeReview.js';
import { IntakeOutcome } from './IntakeOutcome.js';

function githubUrl(value: string | null) {
  if (!value) return undefined;
  try {
    const url = new URL(value);
    return url.protocol === 'https:' && url.hostname === 'github.com' ? url.href : undefined;
  } catch {
    return undefined;
  }
}
export function TicketDetail({
  id,
  owner = false,
  onBack,
  onChanged,
}: {
  id: string;
  owner?: boolean;
  onBack?: () => void;
  onChanged?: () => void;
}) {
  const result = useTicket(id);
  const changed = () => {
    result.refresh();
    onChanged?.();
  };
  return (
    <section className="support-detail" aria-label="Ticket details">
      <DataState data={result.data} error={result.error} retry={result.refresh}>
        {(detail) => (
          <TicketContent
            key={detail.ticket.id}
            detail={detail}
            owner={owner}
            onBack={onBack}
            onChanged={changed}
          />
        )}
      </DataState>
    </section>
  );
}
function TicketContent({
  detail,
  owner,
  onBack,
  onChanged,
}: {
  detail: SupportDetail;
  owner: boolean;
  onBack?: () => void;
  onChanged: () => void;
}) {
  const { ticket } = detail;
  const pr = githubUrl(ticket.prUrl);
  const [body, setBody] = useState('');
  const reply = useRef<HTMLTextAreaElement>(null);
  const [target, setTarget] = useState<'reporter' | 'agent'>('reporter');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const focusReply = (nextTarget: 'reporter' | 'agent') => {
    setTarget(nextTarget);
    reply.current?.focus();
    reply.current?.scrollIntoView({ block: 'center' });
  };
  const mutate = async (work: () => Promise<unknown>) => {
    setBusy(true);
    setError('');
    try {
      await work();
      onChanged();
      return true;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'The action could not be completed.');
      return false;
    } finally {
      setBusy(false);
    }
  };
  const send = async (event: FormEvent) => {
    event.preventDefault();
    if (!busy && body.trim() && (await mutate(() => sendMessage(ticket.id, body.trim(), target))))
      setBody('');
  };
  return (
    <>
      {onBack && (
        <button className="text-button support-back" onClick={onBack}>
          <ArrowLeft size={16} />
          All tickets
        </button>
      )}
      <div className="support-detail-heading">
        <span className="support-eyebrow">
          {kindLabel(ticket.kind)} · {ticket.id}
        </span>
        <h2>{ticket.title}</h2>
        <TicketStatus ticket={ticket} />
        <p className="muted">
          {ticket.reporterName}
          {ticket.dspName ? ` · ${ticket.dspName}` : ''} · {when(ticket.createdAt)}
        </p>
      </div>
      <div className="support-progress" role="status">
        <span className="support-progress-dot" aria-hidden="true" />
        <div>
          <strong>{ticket.progress || 'Your report has been received.'}</strong>
          {ticket.status === 'merged' && (
            <p>
              The change is merged. It becomes available in production with a published release.
            </p>
          )}
          {ticket.status === 'resolved' && ticket.releasedVersion && (
            <p>Available in production · {ticket.releasedVersion}</p>
          )}
        </div>
      </div>
      <section className="support-section">
        <h3>Original report</h3>
        <p className="support-body">{detail.description}</p>
        <div className="support-attachment-grid">
          {detail.attachments.map((attachment) => (
            <a
              href={attachmentUrl(attachment.id)}
              target="_blank"
              rel="noreferrer"
              key={attachment.id}
            >
              <img src={attachmentUrl(attachment.id)} alt={attachment.name} loading="lazy" />
              <span>
                {attachment.name}
                {attachment.private ? ' · Private evidence' : ''}
              </span>
            </a>
          ))}
        </div>
      </section>
      <IntakeOutcome
        status={ticket.status}
        onReply={() => focusReply('reporter')}
        disabled={busy}
      />
      {owner && detail.intake && <IntakeReview intake={detail.intake} />}
      {owner && (
        <Proposal
          ticket={ticket}
          proposals={detail.proposals}
          onChanged={onChanged}
          onAsk={() => focusReply('agent')}
        />
      )}
      {detail.evidence.length > 0 && (
        <section className="support-section">
          <h3>Verification evidence</h3>
          <div className="support-evidence-list">
            {detail.evidence.map((evidence) => (
              <article className="support-evidence" key={evidence.id}>
                <div
                  className={evidence.passed ? 'support-evidence-pass' : 'support-evidence-fail'}
                >
                  {evidence.passed ? <CheckCircle2 size={17} /> : <XCircle size={17} />}
                  <strong>{evidence.criterion}</strong>
                </div>
                <p className="support-body">{evidence.outcome}</p>
                <details>
                  <summary>Steps and source</summary>
                  <ol>
                    {evidence.steps.map((step, index) => (
                      <li key={index}>{step}</li>
                    ))}
                  </ol>
                  <p className="muted">
                    {evidence.phase} · {evidence.sourceSha.slice(0, 12)} ·{' '}
                    {when(evidence.createdAt)}
                  </p>
                  {evidence.attachmentIds.map((attachmentId) => (
                    <a
                      key={attachmentId}
                      href={attachmentUrl(attachmentId)}
                      target="_blank"
                      rel="noreferrer"
                      className="underlined-link"
                    >
                      View evidence attachment
                    </a>
                  ))}
                </details>
              </article>
            ))}
          </div>
        </section>
      )}
      {pr && (
        <a className="support-pr" href={pr} target="_blank" rel="noreferrer">
          <span>
            <strong>View pull request</strong>
            <span className="muted">
              {ticket.mergeSha
                ? 'Merged'
                : ticket.headSha
                  ? `Commit ${ticket.headSha.slice(0, 12)}`
                  : 'Implementation progress'}
            </span>
          </span>
          <ExternalLink size={17} />
        </a>
      )}
      <section className="support-section">
        <h3>Conversation</h3>
        {!detail.messages.length ? (
          <p className="muted">Updates and requests for information will appear here.</p>
        ) : (
          <ol className="support-conversation" aria-label="Ticket conversation">
            {detail.messages.map((message) => (
              <li key={message.id} className={message.private ? 'support-message-private' : ''}>
                <div>
                  <strong>{message.author}</strong>
                  <span className="muted">{when(message.createdAt)}</span>
                  {message.private && (
                    <span className="support-private-label">
                      <LockKeyhole size={12} />
                      Owner and agent only
                    </span>
                  )}
                </div>
                <p className="support-body">{message.body}</p>
              </li>
            ))}
          </ol>
        )}
        <form onSubmit={send} className="support-reply">
          {owner && (
            <label>
              Send to
              <select
                value={target}
                onChange={(event) => setTarget(event.target.value as 'reporter' | 'agent')}
                disabled={busy}
              >
                <option value="reporter">Reporter · Shared conversation</option>
                <option value="agent">Agent · Private question</option>
              </select>
            </label>
          )}
          <label>
            {owner && target === 'agent' ? 'Question for the agent' : 'Reply'}
            <textarea
              ref={reply}
              rows={3}
              maxLength={20000}
              required
              value={body}
              onChange={(event) => setBody(event.target.value)}
              placeholder={
                target === 'agent'
                  ? 'Ask about the analysis or another approach…'
                  : ticket.status === 'unclear_bug'
                    ? 'What did you do, what happened and what did you expect?'
                    : ticket.status === 'unclear_feature'
                      ? 'What would you like to do, and how would it help?'
                      : 'Add details or answer a question…'
              }
              disabled={busy}
            />
          </label>
          <ErrorBox message={error} />
          <div className="support-reply-actions">
            {['resolved', 'merged', 'denied'].includes(ticket.status) && (
              <button
                type="button"
                disabled={busy}
                onClick={() => void mutate(() => reopenTicket(ticket.id))}
              >
                Reopen ticket
              </button>
            )}
            <button className="primary" disabled={busy || !body.trim()}>
              <Send size={15} />
              {busy ? 'Sending…' : target === 'agent' ? 'Ask agent' : 'Send reply'}
            </button>
          </div>
        </form>
      </section>
    </>
  );
}
