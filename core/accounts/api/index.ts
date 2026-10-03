import type { Narrow } from '../../foundation/api/narrow.js';
import type { Feature } from '../../tenancy/api/index.js';
import type { Role as GeneratedRole } from '../../../shared/contracts/generated/Role';
import type { DspView as GeneratedDspView } from '../../../shared/contracts/generated/DspView';
export type { PublicUser as User } from '../../../shared/contracts/generated/PublicUser';
export type { Member as Membership } from '../../../shared/contracts/generated/Member';
import { permissions } from '../../../shared/contracts/generated/access-catalog.js';
export { permissions };
export type Permission = (typeof permissions)[number];
/** `members` and `invitations` are counted by the role list only; a saved role has null. */
export type Role = Narrow<GeneratedRole, { permissions: Permission[] }>;
export type { DspSummary } from '../../../shared/contracts/generated/DspSummary';
export type { SessionResponse as SessionView } from '../../../shared/contracts/generated/SessionResponse';
export type { RuntimeSource } from '../../../shared/contracts/generated/RuntimeSource';
type ViewRole = Pick<Role, 'id' | 'name' | 'owner'>;
export type DspView = Narrow<
  GeneratedDspView,
  { permissions: Permission[]; features: Feature[]; role: ViewRole; roles?: ViewRole[] }
>;
export type { DspProfile } from '../../../shared/contracts/generated/DspProfile';
export type { AccountSession } from '../../../shared/contracts/generated/AccountSession';
export type { SecurityStatus } from '../../../shared/contracts/generated/SecurityStatus';
export type { PasskeySummary } from '../../../shared/contracts/generated/PasskeySummary';
export type { AuthenticatorSetup } from '../../../shared/contracts/generated/AuthenticatorSetup';
