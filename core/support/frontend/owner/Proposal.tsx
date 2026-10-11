import { useState } from 'react';
import { Check, GitPullRequest, MessageCircle, PauseCircle, X } from 'lucide-react';
import { decideTicket, resumeTicket } from '../../api/client.js';
import type { SupportProposal, SupportTicket } from '../../api/index.js';
import { ErrorBox } from '../../../shell/frontend/ui/index.js';

function Words({
  title,
  items,
  ordered = false,
}: {
  title: string;
  items: string[];
  ordered?: boolean;
}) {
  if (!items.length) return null;
  const List = ordered ? 'ol' : 'ul';
  return (
    <div className="support-proposal-section">
      <h4>{title}</h4>
      <List>
        {items.map((item, index) => (
          <li key={index}>{item}</li>
        ))}
      </List>
    </div>
  );
}
export function Proposal({
  ticket,
  proposals,
  onChanged,
  onAsk,
}: {
  ticket: SupportTicket;
  proposals: SupportProposal[];
  onChanged: () => void;
  onAsk: () => void;
}) {
  const [selected, setSelected] = useState<number>();
  const [decision, setDecision] = useState<'approve' | 'deny'>();
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const latest =
    proposals.find((proposal) => proposal.version === ticket.proposalVersion) ?? proposals.at(-1);
  const proposal = proposals.find((proposal) => proposal.version === selected) ?? latest;
  const current = proposal?.version === ticket.proposalVersion;
  const canDecide = Boolean(current && proposal && ticket.status === 'awaiting_approval');
  const run = async (work: () => Promise<unknown>) => {
    setBusy(true);
    setError('');
    try {
      await work();
      setDecision(undefined);
      setNote('');
      onChanged();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'The decision could not be saved.');
    } finally {
      setBusy(false);
    }
  };
  if (!proposal)
    return (
      <section className="support-section support-analysis-pending">
        <GitPullRequest size={20} />
        <h3>Investigation in progress</h3>
        <p className="muted">
          Browser observations and a proposed plan will appear here when the agent completes its
          analysis.
        </p>
        {ticket.status === 'waiting' && (
          <>
            <ErrorBox message={error} />
            <button disabled={busy} onClick={() => void run(() => resumeTicket(ticket.id))}>
              <PauseCircle size={15} />
              Resume investigation
            </button>
          </>
        )}
      </section>
    );
  return (
    <section className="support-section support-proposal">
      <div className="support-section-heading">
        <h3>Agent analysis</h3>
        <label className="support-version">
          Proposal
          <select
            aria-label="Proposal version"
            value={proposal.version}
            onChange={(event) => {
              setSelected(Number(event.target.value));
              setDecision(undefined);
            }}
          >
            {proposals.map((item) => (
              <option value={item.version} key={item.version}>
                Version {item.version}
                {item.version === ticket.proposalVersion ? ' · Latest' : ''}
              </option>
            ))}
          </select>
        </label>
      </div>
      {!current && (
        <p className="notice">
          This is an earlier proposal. Decisions apply to the latest version.
        </p>
      )}
      <div className="support-reproduction">
        <strong>Browser reproduction</strong>
        <p className="support-body">{proposal.reproduction}</p>
      </div>
      <Words title="Observed" items={proposal.observations} />
      <Words title="Uncertainty" items={proposal.uncertainties} />
      <div className="support-proposal-section">
        <h4>Analysis</h4>
        <p className="support-body">{proposal.analysis}</p>
      </div>
      <Words title="Implementation plan" items={proposal.plan} ordered />
      <Words title="Acceptance criteria" items={proposal.acceptanceCriteria} />
      <details className="support-proposal-more">
        <summary>Verification, alternatives and risks</summary>
        <Words title="Verification plan" items={proposal.verification} />
        <Words
          title="Read-only production checks after release"
          items={proposal.productionVerification}
        />
        <Words title="Alternatives" items={proposal.alternatives} />
        <Words title="Affected areas" items={proposal.affectedAreas} />
        <Words title="Risks" items={proposal.risks} />
        <p className="muted">Analysis source · {proposal.sourceSha.slice(0, 12)}</p>
      </details>
      {ticket.approvedVersion === proposal.version && (
        <p className="support-approved">
          <Check size={16} />
          Version {proposal.version} approved for implementation through merge.
        </p>
      )}
      <ErrorBox message={error} />
      {canDecide && !decision && (
        <div className="support-decision-actions">
          <button onClick={onAsk} disabled={busy}>
            <MessageCircle size={15} />
            Ask a question
          </button>
          <button
            className="danger"
            onClick={() => {
              setSelected(proposal.version);
              setDecision('deny');
            }}
            disabled={busy}
          >
            <X size={15} />
            Deny
          </button>
          <button
            className="primary"
            onClick={() => {
              setSelected(proposal.version);
              setDecision('approve');
            }}
            disabled={busy}
          >
            <Check size={16} />
            Approve proposal
          </button>
        </div>
      )}
      {canDecide && decision && (
        <form
          className="support-decision"
          onSubmit={(event) => {
            event.preventDefault();
            void run(() => decideTicket(ticket.id, decision, proposal.version, note.trim()));
          }}
        >
          <h4>
            {decision === 'approve'
              ? `Approve version ${proposal.version}`
              : `Deny version ${proposal.version}`}
          </h4>
          <p>
            {decision === 'approve'
              ? 'The agent implements this plan, verifies it in a browser, opens a pull request and keeps correcting issues until it merges. Production receives the change with a published release.'
              : 'The reporter will be notified of your decision.'}
          </p>
          <label>
            {decision === 'approve' ? 'Approval note (optional)' : 'Reason for denying'}
            <textarea
              rows={2}
              maxLength={20000}
              value={note}
              onChange={(event) => setNote(event.target.value)}
              disabled={busy}
              required={decision === 'deny'}
            />
          </label>
          <div className="support-decision-actions">
            <button type="button" onClick={() => setDecision(undefined)} disabled={busy}>
              Cancel
            </button>
            <button
              className={decision === 'approve' ? 'primary' : 'danger'}
              disabled={busy || (decision === 'deny' && !note.trim())}
            >
              {busy
                ? 'Saving…'
                : decision === 'approve'
                  ? 'Approve and implement'
                  : 'Deny proposal'}
            </button>
          </div>
        </form>
      )}
      {ticket.status === 'waiting' && (
        <button disabled={busy} onClick={() => void run(() => resumeTicket(ticket.id))}>
          Resume work
        </button>
      )}
    </section>
  );
}
