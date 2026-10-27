import type { ReactNode } from 'react';
import './list.css';

// The pieces a list of conversations, contacts or drivers is made of: its rows, each section's
// heading, and what it says when it holds nobody.

/** A list's rows. */
export function Rows({ children }: { children: ReactNode }) {
  return <ul className="convos">{children}</ul>;
}

/** The heading of a section of a list, with how many it holds, when that's told. */
export function ListHead({ children, count }: { children: ReactNode; count?: number }) {
  return (
    <h2 className="list-head">
      {children}
      {count !== undefined && <span>{count}</span>}
    </h2>
  );
}

/** What a list says while it holds nobody, or why it couldn't be read. */
export function ListEmpty({ children }: { children: ReactNode }) {
  return <p className="list-empty">{children}</p>;
}
