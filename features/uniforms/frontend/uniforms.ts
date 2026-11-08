import type {
  Uniform,
  UniformAdjustment,
  UniformInventory,
  UniformUpdates,
  UniformVariant,
} from '../api/index.js';

export const uniformTotal = (uniform: Uniform) =>
  uniform.variants.reduce((sum, v) => sum + v.quantity, 0);

/**
 * The revision each request's count change was made at, as it came back, live or as its answer: a
 * page that asked can tell its own change once it's in what it was read with.
 */
export const madeAt = new Map<string, number>();
const noteMade = (adjustments: UniformAdjustment[]) => {
  for (const change of adjustments)
    if (change.requestId) madeAt.set(change.requestId, change.revision);
};

const adjustmentOf = (variant: UniformVariant): UniformAdjustment => ({
  variantId: variant.id,
  revision: variant.revision,
  quantity: variant.quantity,
  requestId: variant.requestId,
});

/** A delayed mutation response can never move a quantity backwards. */
export function applyUniformAdjustments(
  inventory: UniformInventory,
  adjustments: UniformAdjustment[],
): UniformInventory {
  noteMade(adjustments);
  const latest = new Map<string, UniformAdjustment>();
  for (const change of adjustments) {
    if (change.revision > (latest.get(change.variantId)?.revision ?? -1))
      latest.set(change.variantId, change);
  }
  return {
    ...inventory,
    uniforms: inventory.uniforms.map((uniform) => {
      if (!uniform.variants.some((v) => (latest.get(v.id)?.revision ?? -1) > v.revision))
        return uniform;
      return {
        ...uniform,
        variants: uniform.variants.map((variant) => {
          const change = latest.get(variant.id);
          return change && change.revision > variant.revision
            ? {
                ...variant,
                quantity: change.quantity,
                revision: change.revision,
                requestId: change.requestId,
              }
            : variant;
        }),
      };
    }),
  };
}

/** Snapshots replace the catalog but keep acknowledgements newer than that snapshot. */
export function applyUniformSnapshot(
  current: UniformInventory | undefined,
  next: UniformInventory,
): UniformInventory {
  // Each count's last change, as the snapshot has it, tells the page that asked for it.
  noteMade(next.uniforms.flatMap((u) => u.variants).map(adjustmentOf));
  if (!current) return next;
  if (next.revision < current.revision) return current;
  const newer = current.uniforms
    .flatMap((u) => u.variants)
    .filter((v) => v.revision > next.revision)
    .map(adjustmentOf);
  return applyUniformAdjustments(next, newer);
}
export function applyUniformUpdates(
  current: UniformInventory | undefined,
  update: UniformUpdates,
): UniformInventory | undefined {
  if (update.inventory) return applyUniformSnapshot(current, update.inventory);
  if (!current) return current;
  return {
    ...applyUniformAdjustments(current, update.adjustments),
    revision: Math.max(current.revision, update.revision),
  };
}

export function groupUniforms(uniforms: Uniform[]): Map<string, Uniform[]> {
  const groups = new Map<string, Uniform[]>();
  for (const uniform of uniforms) {
    const group = groups.get(uniform.category);
    if (group) group.push(uniform);
    else groups.set(uniform.category, [uniform]);
  }
  return groups;
}
