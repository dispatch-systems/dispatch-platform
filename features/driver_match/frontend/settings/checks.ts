// The questions the Driver Match tab asks: are these two one person?
import type { Driver, DriverId, DriverPair } from '../../api/index.js';
import { confirmDriver, keepDriversApart, mergeDrivers, splitDriver } from '../../api/client.js';
import { evidenceSupports, evidenceText } from '../driver-match.js';

/** One side of a question: a person, or the IDs on one side of a name variant. */
export type CheckSide = Pick<Driver, 'code' | 'name' | 'role' | 'photo' | 'ids'>;

/**
 * Two people a match suggested may be one, or an ID a name written another way joined to
 * someone, which nobody has confirmed: either way, are they one person?
 */
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

function pairCheck(pair: DriverPair): Check {
  const { person, candidate } = pair;
  return {
    key: `${person.code}:${candidate.code}`,
    sides: [person, candidate],
    likely: pair.strength === 'strong',
    reasons: pair.evidence.map((evidence) => ({
      text: evidenceText(evidence),
      supports: evidenceSupports(evidence),
    })),
    codes: [person.code, candidate.code],
    answer: (same) =>
      same
        ? mergeDrivers(person.code, candidate.code)
        : keepDriversApart(person.code, candidate.code),
  };
}

/** A photo belongs to whichever side holds the ID it is held under. */
const pictured = (ids: DriverId[]) => ids.some((id) => id.photo);

function variantCheck(driver: Driver, id: DriverId): Check {
  const side = (ids: DriverId[]): CheckSide => ({
    ...driver,
    name: ids[0]?.name || driver.name,
    ids,
    photo: pictured(ids) ? driver.photo : null,
  });
  return {
    key: `${driver.code}:${id.source}:${id.id}`,
    sides: [side([id]), side(driver.ids.filter((other) => other !== id))],
    likely: true,
    reasons: [{ text: 'Similar name', supports: true }],
    codes: [driver.code],
    answer: (same) =>
      same
        ? confirmDriver(driver.code, id.source, id.id)
        : splitDriver(driver.code, id.source, id.id),
  };
}

/** Every question: the pairs, the strongest first, then the name variants of anyone still there. */
export const checkList = (review: DriverPair[], drivers: Driver[]): Check[] => [
  ...review.map(pairCheck),
  ...drivers
    .filter((driver) => driver.role !== 'former')
    .flatMap((driver) =>
      driver.ids.filter((id) => id.linkedBy === 'variant').map((id) => variantCheck(driver, id)),
    ),
];
