export const THEMES = ['light', 'dark', 'system'] as const;

export type Theme = (typeof THEMES)[number];

const THEME_KEY = 'theme';
const THEME_CHANGE_EVENT = 'themechange';

export function getStoredTheme(): Theme {
  try {
    const value = localStorage.getItem(THEME_KEY);
    return value === 'light' || value === 'dark' ? value : 'system';
  } catch {
    return 'system';
  }
}

function isDarkTheme(theme: Theme) {
  return (
    theme === 'dark' ||
    (theme === 'system' &&
      window.matchMedia('(prefers-color-scheme: dark)').matches)
  );
}

export function getThemeRootProps() {
  if (typeof window === 'undefined') {
    return {};
  }

  const isDark = isDarkTheme(getStoredTheme());
  return { className: isDark ? 'dark' : undefined };
}

export function setTheme(theme: Theme) {
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {}

  const root = document.documentElement;
  const isDark = isDarkTheme(theme);

  root.classList.add('theme-switching');
  root.classList.toggle('dark', isDark);
  requestAnimationFrame(() =>
    requestAnimationFrame(() => root.classList.remove('theme-switching'))
  );

  window.dispatchEvent(new Event(THEME_CHANGE_EVENT));
}

export function subscribeToTheme(onChange: () => void) {
  window.addEventListener(THEME_CHANGE_EVENT, onChange);
  window.addEventListener('storage', onChange);

  return () => {
    window.removeEventListener(THEME_CHANGE_EVENT, onChange);
    window.removeEventListener('storage', onChange);
  };
}

function initTheme() {
  const media = window.matchMedia('(prefers-color-scheme: dark)');

  const getTheme = () => {
    try {
      const value = localStorage.getItem('theme');
      return value === 'light' || value === 'dark' ? value : 'system';
    } catch {
      return 'system';
    }
  };

  const applyTheme = () => {
    const theme = getTheme();
    const isDark = theme === 'dark' || (theme === 'system' && media.matches);
    document.documentElement.classList.toggle('dark', isDark);
  };

  applyTheme();
  media.addEventListener('change', () => {
    if (getTheme() === 'system') {
      applyTheme();
    }
  });
  window.addEventListener('storage', applyTheme);
}

export const THEME_SCRIPT = `(${initTheme.toString()})()`;
