import type { ConductOverview, ConductStanding } from '../api/index.js';
import { tiers } from './tiers.js';

/** One derived index per immutable response, released with that response. */
const indexes = new WeakMap<ConductOverview, ReturnType<typeof build>>();
function build(overview: ConductOverview) {
  const standings = new Map<string, ConductStanding[]>();
  for (const standing of overview.standings) {
    const held = standings.get(standing.person) ?? [];
    held.push(standing);
    standings.set(standing.person, held);
  }
  for (const held of standings.values())
    held.sort((a, b) => tiers.indexOf(b.tier) - tiers.indexOf(a.tier));
  return {
    people: new Map(overview.people.map((p) => [p.code, p])),
    categories: new Map(overview.categories.map((c) => [c.id, c])),
    standings,
  };
}
export function indexOf(overview: ConductOverview) {
  let index = indexes.get(overview);
  if (!index) {
    index = build(overview);
    indexes.set(overview, index);
  }
  return index;
}
