import { NavLink } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { cn } from '~/utils/classname';
import type { NavItem } from '~/utils/navigation';

import { SidebarTooltip } from './sidebar-tooltip';

type SidebarNavLinkProps = {
  item: NavItem;
  isCollapsed: boolean;
  count?: number;
};

export function SidebarNavLink(props: SidebarNavLinkProps) {
  const { item, isCollapsed, count } = props;

  return (
    <SidebarTooltip
      label={item.label}
      disabled={!isCollapsed}
      trigger={
        <NavLink
          to={item.path}
          prefetch="intent"
          className={cn(
            'group flex h-7.5 shrink-0 items-center gap-2.5 rounded-lg pr-2 pl-2.5 text-body font-medium text-sidebar-text outline-none hover:bg-white/5 focus-visible:ring-2 focus-visible:ring-white/30 aria-[current=page]:bg-sidebar-item-active aria-[current=page]:text-white',
            isCollapsed && 'ml-0.75 w-7.5 rounded-md pr-0 pl-1.75'
          )}
        >
          <Icon
            name={item.icon}
            className="text-sidebar-text-muted group-aria-[current=page]:text-white"
          />
          <span className={cn('flex-1 truncate', isCollapsed && 'sr-only')}>
            {item.label}
          </span>
          {!!count && !isCollapsed && (
            <span className="flex h-4.5 items-center rounded-xs bg-sidebar-badge px-1.5 text-caption font-medium text-sidebar-text">
              {count}
            </span>
          )}
        </NavLink>
      }
    />
  );
}
