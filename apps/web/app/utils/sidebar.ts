import { readStorage, saveStorage } from '~/utils/storage';

const COLLAPSED_KEY = 'sidebar-collapsed';

export function readSidebarCollapsed() {
  return readStorage<boolean | null>(COLLAPSED_KEY, null) ?? window.matchMedia('(max-width: 1023px)').matches;
}

export function saveSidebarCollapsed(isCollapsed: boolean) {
  saveStorage(COLLAPSED_KEY, isCollapsed);
}
