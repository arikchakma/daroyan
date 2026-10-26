import { useHotkey, useHotkeySequences } from '@tanstack/react-hotkeys';
import type { Hotkey } from '@tanstack/react-hotkeys';
import { useQueries } from '@tanstack/react-query';
import { useRef, useState, useSyncExternalStore } from 'react';
import { useLocation, useNavigate } from 'react-router';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { Command } from '~/components/ui/command';
import { Dialog } from '~/components/ui/dialog';
import { Kbd } from '~/components/ui/kbd';
import type { CommandAction, CommandGroup } from '~/hooks/use-command-actions';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { logout } from '~/lib/auth';
import { toast } from '~/lib/toast';
import { listAlertRulesOptions } from '~/queries/alerts';
import { listCronsOptions } from '~/queries/crons';
import { listIncidentsOptions } from '~/queries/incidents';
import { listServersOptions } from '~/queries/servers';
import { listStatusPagesOptions } from '~/queries/status-pages';
import { listUptimeChecksOptions } from '~/queries/uptime';
import {
  NAV_ITEMS,
  SETTINGS_ITEMS,
  SETTINGS_NAV_ITEM,
} from '~/utils/navigation';
import type { NavItem } from '~/utils/navigation';
import { getStoredTheme, setTheme, subscribeToTheme } from '~/utils/theme';

import { ShortcutsDialog } from './shortcuts-dialog';

const RECORD_LIMIT = 5;

type CommandMenuProps = {
  open: boolean;
  pageGroups: CommandGroup[];
  onOpenChange: (open: boolean) => void;
};

export function CommandMenu(props: CommandMenuProps) {
  const { open, pageGroups, onOpenChange } = props;

  const navigate = useNavigate();
  const { pathname } = useLocation();
  const isAdmin = useIsAdmin();
  const theme = useSyncExternalStore(
    subscribeToTheme,
    getStoredTheme,
    () => 'system'
  );

  const [query, setQuery] = useState('');
  const [isShortcutsOpen, setIsShortcutsOpen] = useState(false);
  const hasRunActionRef = useRef(false);

  const section = [...NAV_ITEMS, ...SETTINGS_ITEMS, SETTINGS_NAV_ITEM]
    .filter(
      (item) =>
        pathname === item.path ||
        (item.path !== '/' && pathname.startsWith(`${item.path}/`))
    )
    .sort((a, b) => b.path.length - a.path.length)[0];
  const pageLabel = section?.label ?? 'This Page';
  const groups = pageGroups.map((group) =>
    group.label === 'Actions' ? { ...group, label: pageLabel } : group
  );
  const search = query.trim().toLowerCase();
  const isSearching = open && search.length > 0;

  useHotkeySequences(
    NAV_ITEMS.flatMap((item) =>
      item.shortcut
        ? [
            {
              sequence: ['G', item.shortcut] satisfies Hotkey[],
              callback: () => void navigate(item.path),
            },
          ]
        : []
    )
  );

  useHotkey('Shift+/', () => setIsShortcutsOpen(true));

  const lazy = { enabled: isSearching, throwOnError: false };
  const [servers, checks, crons, rules, incidents, statusPages] = useQueries({
    queries: [
      { ...listServersOptions(), ...lazy },
      { ...listUptimeChecksOptions(), ...lazy },
      { ...listCronsOptions(), ...lazy },
      { ...listAlertRulesOptions(), ...lazy },
      { ...listIncidentsOptions(), ...lazy },
      { ...listStatusPagesOptions(), ...lazy },
    ],
  });

  const changeOpen = (next: boolean) => {
    if (!next) {
      setQuery('');
    }

    onOpenChange(next);
  };

  const toCommand = (item: NavItem): CommandAction => ({
    label: item.label,
    icon: item.icon,
    shortcut: item.shortcut && `G ${item.shortcut}`,
    onSelect: () => void navigate(item.path),
  });

  const toRecords = (
    records: { id: string; label: string }[] | undefined,
    icon: IconName,
    path: string
  ) =>
    (records ?? [])
      .filter((record) => record.label.toLowerCase().includes(search))
      .slice(0, RECORD_LIMIT)
      .map(
        (record): CommandAction => ({
          label: record.label,
          icon,
          onSelect: () => void navigate(`${path}/${record.id}`),
        })
      );

  const named = (records: { id: string; name: string }[] | undefined) =>
    records?.map((record) => ({ id: record.id, label: record.name }));

  const recordGroups = isSearching
    ? [
        {
          label: 'Servers',
          path: '/servers',
          actions: toRecords(named(servers.data), 'server', '/servers'),
        },
        {
          label: 'Uptime Checks',
          path: '/uptime',
          actions: toRecords(named(checks.data), 'activity', '/uptime'),
        },
        {
          label: 'Crons',
          path: '/crons',
          actions: toRecords(named(crons.data), 'clock', '/crons'),
        },
        {
          label: 'Alert Rules',
          path: '/alerts',
          actions: toRecords(named(rules.data), 'bell', '/alerts'),
        },
        {
          label: 'Incidents',
          path: '/incidents',
          actions: toRecords(
            incidents.data?.map((incident) => ({
              id: incident.id,
              label: incident.title,
            })),
            'flag',
            '/incidents'
          ),
        },
        {
          label: 'Status Pages',
          path: '/status-pages',
          actions: toRecords(
            named(statusPages.data),
            'globe',
            '/status-pages'
          ),
        },
      ].sort(
        (a, b) =>
          Number(b.path === section?.path) - Number(a.path === section?.path)
      )
    : [];

  const builtInGroups: {
    label: string;
    actions: (CommandAction | false)[];
  }[] = [
    {
      label: 'Actions',
      actions: [
        isAdmin && {
          label: 'Add Server',
          icon: 'plus',
          keywords: 'install agent',
          onSelect: () => void navigate('/servers/new'),
        },
        {
          label: 'New Uptime Check',
          icon: 'activity',
          keywords: 'monitor',
          onSelect: () => void navigate('/uptime/new'),
        },
        {
          label: 'New Cron',
          icon: 'clock',
          keywords: 'heartbeat job',
          onSelect: () => void navigate('/crons/new'),
        },
        {
          label: 'New Alert Rule',
          icon: 'bell',
          onSelect: () => void navigate('/alerts/new'),
        },
        {
          label: 'Declare Incident',
          icon: 'flag',
          onSelect: () => void navigate('/incidents?new=incident'),
        },
        {
          label: 'Schedule Maintenance',
          icon: 'calendar',
          onSelect: () => void navigate('/incidents?new=maintenance'),
        },
        {
          label: 'New Status Page',
          icon: 'globe',
          onSelect: () => void navigate('/status-pages/new'),
        },
        {
          label: 'Copy Page Link',
          icon: 'link',
          keywords: 'url share',
          onSelect: () =>
            void navigator.clipboard
              .writeText(window.location.href)
              .then(
                () => toast.success('Link copied'),
                () => toast.error('Could not copy link')
              ),
        },
      ],
    },
    { label: 'Jump To', actions: NAV_ITEMS.map(toCommand) },
    {
      label: 'Preferences',
      actions: [
        {
          label: 'Light Theme',
          icon: 'sun',
          keywords: 'appearance mode',
          isChecked: theme === 'light',
          onSelect: () => setTheme('light'),
        },
        {
          label: 'Dark Theme',
          icon: 'moon',
          keywords: 'appearance mode',
          isChecked: theme === 'dark',
          onSelect: () => setTheme('dark'),
        },
        {
          label: 'System Theme',
          icon: 'monitor',
          keywords: 'appearance mode auto',
          isChecked: theme === 'system',
          onSelect: () => setTheme('system'),
        },
      ],
    },
    {
      label: 'Settings',
      actions: [SETTINGS_NAV_ITEM, ...SETTINGS_ITEMS].map(toCommand),
    },
    {
      label: 'Account',
      actions: [
        isAdmin && {
          label: 'Invite Members',
          icon: 'user-plus',
          onSelect: () => void navigate('/settings/members'),
        },
        {
          label: 'Keyboard Shortcuts',
          icon: 'command',
          keywords: 'hotkeys keys help',
          shortcut: '?',
          onSelect: () => setIsShortcutsOpen(true),
        },
        {
          label: 'Sign Out',
          icon: 'log-out',
          keywords: 'log out',
          onSelect: () => void logout(),
        },
      ],
    },
  ];

  const builtInLabels = builtInGroups.map((group) => group.label);
  const seen = new Set<string>();

  const keepFirst = (group: CommandGroup) => ({
    label: group.label,
    actions: group.actions.filter((action) => {
      if (seen.has(action.label)) {
        return false;
      }

      seen.add(action.label);
      return true;
    }),
  });

  const contextLabels = [
    ...new Set(
      groups
        .filter((group) => !builtInLabels.includes(group.label))
        .map((group) => group.label)
    ),
  ];

  const contextGroups = contextLabels
    .map((label) => ({
      label: label || 'Untitled',
      actions: groups
        .filter((group) => group.label === label)
        .flatMap((group) => group.actions),
    }))
    .map(keepFirst);

  const mergedGroups = builtInGroups
    .map((group) => ({
      label: group.label,
      actions: [
        ...groups
          .filter((page) => page.label === group.label)
          .flatMap((page) => page.actions),
        ...group.actions.filter((action) => !!action),
      ],
    }))
    .map(keepFirst);

  const commandGroups = [...contextGroups, ...recordGroups, ...mergedGroups]
    .filter((group) => group.actions.length > 0)
    .map((group) => ({ value: group.label, items: group.actions }));

  const isLoading =
    isSearching &&
    [servers, checks, crons, rules, incidents, statusPages].some(
      (result) => result.isFetching
    );

  return (
    <>
      <Dialog.Root open={open} onOpenChange={changeOpen}>
        <Dialog.Portal>
          <Dialog.Backdrop className="bg-black/20" />
          <Dialog.Popup
            showCloseButton={false}
            finalFocus={() => {
              const shouldRestore = !hasRunActionRef.current;
              hasRunActionRef.current = false;
              return shouldRestore;
            }}
            className="top-30 w-140 translate-y-0 data-ending-style:translate-y-0 data-starting-style:translate-y-0 max-lg:top-3"
          >
            <Dialog.Title className="sr-only">Command Menu</Dialog.Title>
            <Command.Root
              items={commandGroups}
              inline
              open
              autoHighlight="always"
              keepHighlight
              onValueChange={setQuery}
              itemToStringValue={(action: CommandAction) =>
                `${action.label} ${action.keywords ?? ''}`
              }
            >
              <div className="flex h-13 shrink-0 items-center gap-2.5 border-b border-border-row pr-3 pl-4">
                <Icon name="search" className="shrink-0 text-text-tertiary" />
                <Command.Input
                  aria-label="Search or Jump To"
                  placeholder="Search or jump to…"
                />
                <Kbd>esc</Kbd>
              </div>
              <Command.Viewport className="[scroll-padding-block:calc(50%-1.5rem)]">
                <Command.Empty>
                  {isLoading ? 'Searching…' : 'No results.'}
                </Command.Empty>
                <Command.List>
                  {(group: (typeof commandGroups)[number], index: number) => (
                    <Command.Group key={group.value} items={group.items}>
                      {index > 0 && <Command.Separator />}
                      <Command.GroupLabel>{group.value}</Command.GroupLabel>
                      <Command.Collection>
                        {(action: CommandAction, actionIndex: number) => (
                          <Command.Item
                            key={`${action.label}-${actionIndex}`}
                            value={action}
                            onClick={() => {
                              hasRunActionRef.current = true;
                              changeOpen(false);
                              action.onSelect();
                            }}
                          >
                            <Icon name={action.icon} />
                            <span className="min-w-0 flex-1 truncate">
                              {action.label}
                            </span>
                            {action.shortcut && (
                              <Command.Shortcut>
                                {action.shortcut}
                              </Command.Shortcut>
                            )}
                            {action.isChecked && (
                              <Icon
                                name="check"
                                className="text-text-secondary"
                              />
                            )}
                          </Command.Item>
                        )}
                      </Command.Collection>
                    </Command.Group>
                  )}
                </Command.List>
              </Command.Viewport>
              <div className="flex shrink-0 items-center gap-3.5 border-t border-border-row bg-surface-subtle px-3 py-2 text-caption text-text-secondary">
                <span className="flex items-center gap-1">
                  <Kbd className="px-0">
                    <Icon name="arrow-up" />
                  </Kbd>
                  <Kbd className="px-0">
                    <Icon name="arrow-down" />
                  </Kbd>
                  Navigate
                </span>
                <span className="flex items-center gap-1">
                  <Kbd className="px-0">
                    <Icon name="corner-down-left" />
                  </Kbd>
                  Open
                </span>
              </div>
            </Command.Root>
          </Dialog.Popup>
        </Dialog.Portal>
      </Dialog.Root>
      <ShortcutsDialog
        open={isShortcutsOpen}
        onOpenChange={setIsShortcutsOpen}
      />
    </>
  );
}
