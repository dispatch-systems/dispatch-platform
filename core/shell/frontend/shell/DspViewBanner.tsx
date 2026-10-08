import { Check, ChevronDown, Eye } from 'lucide-react';
import type { DspView } from '../../../accounts/api/index.js';
import { Popover } from '../ui/Popover.js';

// The grant a platform owner views a DSP with by default: an owner's permissions, and the
// features hidden from the DSP too.
const PLATFORM_OWNER = 'platform_owner';
/** Who a platform owner is viewing the DSP as. */
const viewer = (view: DspView) =>
  view.role.id === PLATFORM_OWNER
    ? 'Platform Owner'
    : view.role.owner
      ? 'DSP owner'
      : view.role.name;
/** What that view can do and see. */
const viewing = (view: DspView) =>
  view.role.id === PLATFORM_OWNER
    ? 'Full owner access, with the features hidden from the DSP.'
    : view.role.owner
      ? 'Full owner access, as the DSP’s owners have it.'
      : `${view.role.name} access.`;

// Lets a platform owner look through any role the DSP has, its Owner and custom ones
// included, or as the Platform Owner, who also sees what is hidden from the DSP.
function ViewRoleMenu({ view, viewAs }: { view: DspView; viewAs: (roleId?: string) => void }) {
  const choices = [{ id: PLATFORM_OWNER, name: 'Platform Owner' }, ...(view.roles ?? [])];
  return (
    <Popover
      className="view-role-menu"
      label="View as role"
      trigger={
        <>
          {viewer(view) === 'DSP owner'
            ? (view.roles?.find((role) => role.id === view.role.id)?.name ?? view.role.name)
            : viewer(view)}
          <ChevronDown aria-hidden="true" />
        </>
      }
    >
      {choices.map((role) => {
        const current = role.id === view.role.id;
        return (
          <button
            key={role.id}
            aria-current={current || undefined}
            onClick={() => {
              if (!current) viewAs(role.id === PLATFORM_OWNER ? undefined : role.id);
            }}
          >
            <span>{role.name}</span>
            {current && <Check size={16} aria-hidden="true" />}
          </button>
        );
      })}
    </Popover>
  );
}

/**
 * The banner over a DSP a platform owner is viewing: as whom, and the way out. It loads only
 * then, with the menu, so nobody else's first load carries either.
 */
export function DspViewBanner({
  view,
  viewAs,
  exitView,
}: {
  view: DspView;
  viewAs: (roleId?: string) => void;
  exitView: () => void;
}) {
  return (
    <div className="dsp-view-banner" data-sticky-banner role="region" aria-label="DSP viewing mode">
      <Eye aria-hidden="true" />
      <div>
        <strong>
          Viewing {view.dsp.name} as {viewer(view)}
        </strong>
        <span>{viewing(view)} Changes are saved to this DSP.</span>
      </div>
      <ViewRoleMenu view={view} viewAs={viewAs} />
      <button onClick={exitView}>Exit view</button>
    </div>
  );
}
