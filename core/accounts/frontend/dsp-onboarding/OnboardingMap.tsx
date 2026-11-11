import { RouteMap } from '../route-map/RouteMap.js';
import './onboarding-map.css';

export function OnboardingMap({ desktop }: { desktop: boolean }) {
  return <RouteMap desktop={desktop} prefix="onboarding" />;
}
