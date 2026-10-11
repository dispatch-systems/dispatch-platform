import { LockKeyhole } from 'lucide-react';
import type { SupportDetail } from '../../api/index.js';

export function IntakeReview({ intake }: { intake: NonNullable<SupportDetail['intake']> }) {
  return (
    <section className="support-section support-intake-review" aria-label="Private intake review">
      <div className="support-section-heading">
        <h3>Intake review</h3>
        <span className="support-private-label">
          <LockKeyhole size={12} />
          Owner and agent only
        </span>
      </div>
      <p className="support-body">{intake.reason}</p>
      <p className="muted">
        {intake.investigationAttempted ? 'Investigation attempted' : 'Message review completed'}
      </p>
      {intake.findings.length > 0 && (
        <div className="support-proposal-section">
          <h4>Findings</h4>
          <ul>
            {intake.findings.map((finding, index) => (
              <li key={index}>{finding}</li>
            ))}
          </ul>
        </div>
      )}
      {intake.questions.length > 0 && (
        <div className="support-proposal-section">
          <h4>Clarifying questions</h4>
          <ul>
            {intake.questions.map((question, index) => (
              <li key={index}>{question}</li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
