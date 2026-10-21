import { usePicture } from '../../../core/shell/frontend/runtime/usePicture.js';
import { initials, tone } from './workspace-mark.js';
import './workspace-logo.css';

/**
 * A workspace's mark: its logo, the copy Dispatch keeps of the one it has in Slack, from `src`
 * once it nears the screen; the first letters of its `name` in its tone until it comes, or
 * without one. Its owner's `className` sizes it.
 */
export function WorkspaceLogo({
  name,
  src,
  className,
}: {
  name: string;
  src?: string;
  className?: string;
}) {
  const [box, shown, lost] = usePicture<HTMLSpanElement>(src);
  return (
    <span
      ref={box}
      className={className ? `workspace-logo ${className}` : 'workspace-logo'}
      data-tone={shown ? undefined : tone(name)}
      aria-hidden="true"
    >
      {shown ? <img src={shown} alt="" onError={lost} /> : initials(name)}
    </span>
  );
}
