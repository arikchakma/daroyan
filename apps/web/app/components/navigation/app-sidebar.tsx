import { useHotkey } from '@tanstack/react-hotkeys';
import { useQuery } from '@tanstack/react-query';
import { useState, useSyncExternalStore } from 'react';

import { DaroyanLogo } from '~/components/brand/daroyan-logo';
import { Icon } from '~/components/icons/icon';
import { Avatar } from '~/components/ui/avatar';
import { Menu } from '~/components/ui/menu';
import { Tooltip } from '~/components/ui/tooltip';
import { useCommandActions } from '~/hooks/use-command-actions';
import { logout } from '~/lib/auth';
import { meOptions } from '~/queries/auth';
import { listAlertsOptions } from '~/queries/alerts';
import { listIncidentsOptions } from '~/queries/incidents';
import { cn } from '~/utils/classname';
import { formatHotkey } from '~/utils/hotkey';
import { isClosed } from '~/utils/incidents';
import { NAV_SECTIONS, SETTINGS_NAV_ITEM } from '~/utils/navigation';
import { readSidebarCollapsed, saveSidebarCollapsed } from '~/utils/sidebar';
import { getStoredTheme, setTheme, subscribeToTheme } from '~/utils/theme';
import type { Theme } from '~/utils/theme';

import { SidebarNavLink } from './sidebar-nav-link';
import { SidebarTooltip } from './sidebar-tooltip';

type AppSidebarProps = {
  isCompact: boolean;
  onSearch: () => void;
};

export function AppSidebar(props: AppSidebarProps) {
  const { isCompact, onSearch } = props;

  const [isStoredCollapsed, setIsStoredCollapsed] = useState(readSidebarCollapsed);
  const [isCompactExpanded, setIsCompactExpanded] = useState(false);
  const { data: firing } = useQuery(listAlertsOptions('firing'));
  const { data: incidents } = useQuery(listIncidentsOptions());
  const openIncidents = incidents?.filter(
    (incident) => !isClosed(incident.status) && incident.status !== 'scheduled'
  ).length;
  const counts: Record<string, number | undefined> = { '/alerts': firing?.length, '/incidents': openIncidents };
  const isCollapsed = isCompact ? !isCompactExpanded : isStoredCollapsed;

  const toggle = () => {
    if (isCompact) {
      setIsCompactExpanded(!isCompactExpanded);
      return;
    }
    saveSidebarCollapsed(!isStoredCollapsed);
    setIsStoredCollapsed(!isStoredCollapsed);
  };

  useHotkey('Mod+B', toggle);

  useCommandActions('Preferences', [
    {
      label: isCollapsed ? 'Expand Sidebar' : 'Collapse Sidebar',
      icon: 'panel',
      shortcut: formatHotkey('Mod+B'),
      onSelect: toggle,
    },
  ]);

  return (
    <Tooltip.Provider>
      <nav
        aria-label="Main"
        className={cn(
          'flex h-full w-55 shrink-0 flex-col overflow-x-hidden overflow-y-auto bg-sidebar-bg px-2.5 pt-3.5 pb-3 whitespace-nowrap scheme-dark',
          isCollapsed && 'w-14'
        )}
      >
        {isCollapsed ? (
          <SidebarTooltip
            label="Expand Navigation"
            shortcut="Mod+B"
            trigger={
              <button
                type="button"
                aria-label="Expand Navigation"
                onClick={toggle}
                className="group ml-0.5 flex size-8 shrink-0 items-center rounded-sm pl-1.5 text-white outline-none hover:bg-white/10 focus-visible:bg-white/10"
              >
                <DaroyanLogo className="size-5 group-hover:hidden group-focus-visible:hidden" />
                <Icon
                  name="panel"
                  className="ml-0.5 hidden group-hover:block group-focus-visible:block"
                />
              </button>
            }
          />
        ) : (
          <div className="flex h-8 shrink-0 items-center justify-between pr-0.5 pl-2">
            <DaroyanLogo className="size-5" />
            <SidebarTooltip
              label="Collapse Navigation"
              shortcut="Mod+B"
              side="bottom"
              trigger={
                <button
                  type="button"
                  aria-label="Collapse Navigation"
                  onClick={toggle}
                  className="flex size-7 items-center justify-center rounded-sm text-sidebar-label outline-none hover:bg-white/5 focus-visible:ring-2 focus-visible:ring-white/30"
                >
                  <Icon name="panel" />
                </button>
              }
            />
          </div>
        )}

        <SidebarTooltip
          label="Search"
          shortcut="Mod+K"
          disabled={!isCollapsed}
          trigger={
            <button
              type="button"
              onClick={onSearch}
              className={cn(
                'mt-3.5 flex h-8 shrink-0 items-center gap-2 rounded-lg bg-sidebar-search pr-2 pl-2.5 text-sidebar-label outline-none focus-visible:ring-2 focus-visible:ring-white/30',
                isCollapsed &&
                  'ml-0.5 w-8 rounded-md bg-transparent pl-2 text-sidebar-text-muted hover:bg-white/5'
              )}
            >
              <Icon name="search" />
              <span
                className={cn(
                  'flex-1 text-left text-body',
                  isCollapsed && 'sr-only'
                )}
              >
                Search
              </span>
              <span
                className={cn(
                  'text-caption font-medium',
                  isCollapsed && 'hidden'
                )}
              >
                {formatHotkey('Mod+K')}
              </span>
            </button>
          }
        />

        {NAV_SECTIONS.map((section, index) => {
          const labelId = section.label
            ? `sidebar-${section.label.toLowerCase()}`
            : undefined;

          return (
            <div
              key={section.label ?? index}
              role={section.label ? 'group' : undefined}
              aria-labelledby={labelId}
              className="mt-3.5 flex shrink-0 flex-col"
            >
              {section.label && (
                <div className="flex h-6.5 items-center">
                  <span
                    id={labelId}
                    className={cn(
                      'pl-2.5 text-small font-medium text-sidebar-label',
                      isCollapsed && 'sr-only'
                    )}
                  >
                    {section.label}
                  </span>
                  {isCollapsed && (
                    <span className="mx-auto h-px w-5 bg-sidebar-item-active" />
                  )}
                </div>
              )}
              {section.items.map((item) => (
                <SidebarNavLink
                  key={item.path}
                  item={item}
                  isCollapsed={isCollapsed}
                  count={counts[item.path]}
                />
              ))}
            </div>
          );
        })}

        <div className="min-h-3.5 flex-1" />

        <SidebarNavLink item={SETTINGS_NAV_ITEM} isCollapsed={isCollapsed} />

        <div className="flex shrink-0 items-center gap-1">
          <AccountMenu isCollapsed={isCollapsed} />
          {!isCollapsed && (
            <button
              type="button"
              aria-label="Notifications"
              className="flex size-7 shrink-0 items-center justify-center rounded-sm text-sidebar-text-muted outline-none hover:bg-white/5 focus-visible:ring-2 focus-visible:ring-white/30"
            >
              <Icon name="bell" />
            </button>
          )}
        </div>
      </nav>
    </Tooltip.Provider>
  );
}

type AccountMenuProps = {
  isCollapsed: boolean;
};

function AccountMenu(props: AccountMenuProps) {
  const { isCollapsed } = props;

  const { data: me } = useQuery(meOptions());
  const theme = useSyncExternalStore(subscribeToTheme, getStoredTheme, () => 'system');
  const name = me?.name ?? 'Daroyan';

  return (
    <Menu.Root>
      <Menu.Trigger
        className={cn(
          'flex h-9 min-w-0 flex-1 items-center gap-2.5 rounded-lg pr-1 pl-1.75 text-left outline-none hover:bg-white/5 focus-visible:ring-2 focus-visible:ring-white/30',
          isCollapsed && 'ml-0.5 w-9 flex-none pl-1.75'
        )}
      >
        <Avatar id={me?.id ?? name} src={me?.avatar_url} shape="square" />
        <span
          className={cn(
            'min-w-0 flex-1 truncate text-body font-medium text-sidebar-text',
            isCollapsed && 'sr-only'
          )}
        >
          {name}
        </span>
        {!isCollapsed && (
          <Icon name="chevrons-updown" className="text-sidebar-text-muted" />
        )}
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner side="top" align="start">
          <Menu.Popup>
            <Menu.RadioGroup
              value={theme}
              onValueChange={(value: Theme) => setTheme(value)}
            >
              <Menu.GroupLabel>Theme</Menu.GroupLabel>
              <Menu.RadioItem value="light">
                <Icon name="sun" />
                Light
              </Menu.RadioItem>
              <Menu.RadioItem value="dark">
                <Icon name="moon" />
                Dark
              </Menu.RadioItem>
              <Menu.RadioItem value="system">
                <Icon name="monitor" />
                System
              </Menu.RadioItem>
            </Menu.RadioGroup>
            <Menu.Separator />
            <Menu.Item onClick={() => void logout()}>
              <Icon name="log-out" />
              Sign Out
            </Menu.Item>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
