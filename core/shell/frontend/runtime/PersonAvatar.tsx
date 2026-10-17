import { usePicture } from './usePicture.js';

/** The first letters of someone's first two names. */
const initials = (name: string) =>
  (name.match(/[\p{L}\p{N}]+/gu) ?? [])
    .slice(0, 2)
    .map((word) => [...word][0]!.toLocaleUpperCase('en-US'))
    .join('') || '?';

/**
 * A person: the photo at `photo`, an address carrying its version, once it nears the screen,
 * and their initials until it comes or without one. Its owner's `className` sizes and colors it.
 */
export function PersonAvatar({
  name,
  photo,
  className,
}: {
  name: string;
  photo?: string;
  className?: string;
}) {
  const [box, src, lost] = usePicture<HTMLSpanElement>(photo);
  return (
    <span
      ref={box}
      className={className ? `person-avatar ${className}` : 'person-avatar'}
      aria-hidden="true"
    >
      {src ? <img src={src} alt="" onError={lost} /> : initials(name)}
    </span>
  );
}
