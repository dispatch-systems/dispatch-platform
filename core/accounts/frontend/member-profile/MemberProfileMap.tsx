import { RouteMap, type RouteState } from '../route-map/RouteMap.js';
import './member-profile-map.css';

export type MemberProfileRoute = RouteState;

export function MemberProfileMap({
  desktop,
  route = 'live',
}: {
  desktop: boolean;
  route?: MemberProfileRoute;
}) {
  return <RouteMap desktop={desktop} prefix="member-profile" route={route} />;
}
