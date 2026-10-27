import type { Correspondent } from '../../../api/index.js';
import { Face } from '../../shared/Face.js';

/** A group, as large as where it's shown (`size`): two of its people, one over the other, where
 * one person's photo would be. */
export function GroupFaces({
  faces,
  size,
}: {
  faces: readonly Correspondent[];
  size: 'list' | 'head' | 'profile';
}) {
  // One person is shown as they are.
  if (faces.length === 1) return <Face {...faces[0]!} size={size} />;
  return (
    <span className={`group-faces group-faces-${size}`} aria-hidden="true">
      {faces.slice(0, 2).map((person) => (
        <Face key={person.userId} {...person} size="sender" />
      ))}
    </span>
  );
}
