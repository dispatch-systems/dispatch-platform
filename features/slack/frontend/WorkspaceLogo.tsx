import type { ReactNode } from 'react';
import { usePicture } from '../../../core/shell/frontend/runtime/usePicture.js';
import './workspace-logo.css';

/**
 * A workspace's logo, the copy Dispatch keeps of the one it has in Slack, from `src` once it
 * nears the screen; `children` stand in for it until it comes, or without one. Its owner's
 * `className` sizes it.
 */
export function WorkspaceLogo({
  src,
  className,
  children,
}: {
  src?: string;
  className?: string;
  children: ReactNode;
}) {
  const [box, shown, lost] = usePicture<HTMLSpanElement>(src);
  return (
    <span
      ref={box}
      className={className ? `workspace-logo ${className}` : 'workspace-logo'}
      aria-hidden="true"
    >
      {shown ? <img src={shown} alt="" onError={lost} /> : children}
    </span>
  );
}
