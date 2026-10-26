const COLLAPSED_KEY = 'sidebar-collapsed';

export function readSidebarCollapsed() {
  try {
    const stored = localStorage.getItem(COLLAPSED_KEY);
    if (stored !== null) {
      return stored === 'true';
    }
  } catch {}
  return window.matchMedia('(max-width: 1023px)').matches;
}

export function saveSidebarCollapsed(isCollapsed: boolean) {
  try {
    localStorage.setItem(COLLAPSED_KEY, String(isCollapsed));
  } catch {}
}
