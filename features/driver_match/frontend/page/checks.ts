// The questions the Driver Match page asks, as the backend sends them: are these two one person?
import type { Person, PersonId, Question } from '../../api/index.js';
import { confirmLink, keepApart, mergePeople, splitOff } from '../../api/client.js';
import { evidenceText } from '../driver-match.js';

/** One side of a question: a person, or the IDs on one side of a name variant. */
export type CheckSide = Pick<Person, 'code' | 'name' | 'role' | 'photo' | 'ids'>;

/** A question, ready to show and answer: are its two sides one person? */
export interface Check {
  key: string;
  sides: [CheckSide, CheckSide];
  likely: boolean;
  reasons: { text: string; supports: boolean }[];
  /** The people it asks about, so each one's card opens it. */
  codes: string[];
  /** Answers it: one person, or two different people from then on. */
  answer: (same: boolean) => Promise<unknown>;
}

/** A photo belongs to whichever side holds the ID it is held under. */
const pictured = (ids: PersonId[]) => ids.some((id) => id.photo);

/** Each question the backend asks, about the people it names by their codes. */
export function checkList(questions: Question[], people: Person[]): Check[] {
  const byCode = new Map(people.map((person) => [person.code, person]));
  return questions.flatMap((question): Check[] => {
    const person = byCode.get(question.person);
    if (!person) return [];
    if (question.kind === 'pair') {
      const candidate = byCode.get(question.candidate);
      if (!candidate) return [];
      return [
        {
          key: `${person.code}:${candidate.code}`,
          sides: [person, candidate],
          likely: question.strength === 'strong',
          reasons: question.evidence.map((evidence) => ({
            text: evidenceText(evidence),
            supports: evidence.supports,
          })),
          codes: [person.code, candidate.code],
          answer: (same) =>
            same
              ? mergePeople(person.code, candidate.code)
              : keepApart(person.code, candidate.code),
        },
      ];
    }
    const id = person.ids.find((id) => id.source === question.source && id.id === question.id);
    if (!id) return [];
    // The ID on one side, everything else they hold on the other.
    const side = (ids: PersonId[]): CheckSide => ({
      ...person,
      name: ids[0]?.name || person.name,
      ids,
      photo: pictured(ids) ? person.photo : null,
    });
    return [
      {
        key: `${person.code}:${id.source}:${id.id}`,
        sides: [side([id]), side(person.ids.filter((other) => other !== id))],
        likely: true,
        reasons: [{ text: 'Similar name', supports: true }],
        codes: [person.code],
        answer: (same) =>
          same
            ? confirmLink(person.code, id.source, id.id)
            : splitOff(person.code, id.source, id.id),
      },
    ];
  });
}
