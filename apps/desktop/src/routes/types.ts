import type { NavIconName } from '../components/icons';

export type Route = 'overview' | 'usage' | 'sessions' | 'providers' | 'sync' | 'settings';

export const routes: Array<{ id: Route; label: string; icon: NavIconName }> = [
  { id: 'overview', label: 'Overview', icon: 'overview' },
  { id: 'usage', label: 'Usage', icon: 'usage' },
  { id: 'sessions', label: 'Sessions', icon: 'sessions' },
  { id: 'providers', label: 'Providers', icon: 'providers' },
  { id: 'sync', label: 'Sync', icon: 'sync' },
  { id: 'settings', label: 'Settings', icon: 'settings' },
];

export function routeTitle(route: Route) {
  return routes.find((item) => item.id === route)?.label ?? route;
}
