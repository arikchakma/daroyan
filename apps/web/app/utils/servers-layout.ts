const LAYOUT_KEY = 'servers-layout';

export type ServersLayout = 'list' | 'grid';

export function readServersLayout(): ServersLayout {
  try {
    if (localStorage.getItem(LAYOUT_KEY) === 'grid') {
      return 'grid';
    }
  } catch {}

  return 'list';
}

export function saveServersLayout(layout: ServersLayout) {
  try {
    localStorage.setItem(LAYOUT_KEY, layout);
  } catch {}
}
