import type { Hotkey } from '@tanstack/react-hotkeys';

import type { IconName } from '~/components/icons/icon-paths';

export type NavItem = {
  label: string;
  icon: IconName;
  path: string;
  shortcut?: Hotkey;
};

export type NavSection = {
  label?: string;
  items: NavItem[];
};

export const NAV_SECTIONS: NavSection[] = [
  {
    items: [
      { label: 'Overview', icon: 'house', path: '/', shortcut: 'O' },
      { label: 'Servers', icon: 'server', path: '/servers', shortcut: 'S' },
      { label: 'Uptime', icon: 'activity', path: '/uptime', shortcut: 'U' },
      { label: 'Crons', icon: 'clock', path: '/crons', shortcut: 'C' },
      { label: 'Alerts', icon: 'bell', path: '/alerts', shortcut: 'A' },
      { label: 'Incidents', icon: 'flag', path: '/incidents', shortcut: 'I' },
    ],
  },
  {
    label: 'Telemetry',
    items: [
      { label: 'Issues', icon: 'triangle-alert', path: '/issues', shortcut: 'E' },
      { label: 'Traces', icon: 'layers', path: '/traces', shortcut: 'T' },
      { label: 'Logs', icon: 'terminal', path: '/logs', shortcut: 'L' },
    ],
  },
  {
    label: 'Share',
    items: [
      { label: 'Status Pages', icon: 'globe', path: '/status-pages', shortcut: 'P' },
    ],
  },
];

export const SETTINGS_NAV_ITEM: NavItem = {
  label: 'Settings',
  icon: 'settings',
  path: '/settings',
};

export const NAV_ITEMS = NAV_SECTIONS.flatMap((section) => section.items);

export const SETTINGS_ITEMS: NavItem[] = [
  { label: 'Members', icon: 'users', path: '/settings/members' },
  { label: 'Integrations', icon: 'bell', path: '/settings/integrations' },
  { label: 'Agents', icon: 'server', path: '/settings/agents' },
  { label: 'Telemetry', icon: 'layers', path: '/settings/telemetry' },
  { label: 'MCP', icon: 'bot', path: '/settings/mcp' },
  { label: 'Backups', icon: 'database', path: '/settings/backups' },
];
