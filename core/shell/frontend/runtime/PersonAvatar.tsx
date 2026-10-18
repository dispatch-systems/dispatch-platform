import { usePicture } from './usePicture.js';

/** The first letters of someone's first two names. */
const initials = (name: string) =>
  (name.match(/[\p{L}\p{N}]+/gu) ?? [])
    .slice(0, 2)
    .map((word) => [...word][0]!.toLocaleUpperCase('en-US'))
    .join('') || '?';

/** The address of the photo of the person `code` names, at its `version`, which it carries. */
const personPhoto = (code: string, version: string) =>
  `/api/dsp/people/${encodeURIComponent(code)}/photo?v=${version.slice(0, 16)}`;

/**
 * A person: the photo of the person `code` names, at the version `photo` names, once it nears
 * the screen, and their initials until it comes or without one. Its owner's `className` sizes
 * and colors it.
 */
export function PersonAvatar({
  name,
  code,
  photo,
  className,
}: {
  name: string;
  code?: string | null;
  photo?: string | null;
  className?: string;
}) {
  const [box, src, lost] = usePicture<HTMLSpanElement>(
    code && photo ? personPhoto(code, photo) : undefined,
  );
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
