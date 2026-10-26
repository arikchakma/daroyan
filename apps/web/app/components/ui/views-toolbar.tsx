import type { ReactNode } from 'react';

import { Icon } from '~/components/icons/icon';
import { Menu } from '~/components/ui/menu';
import { ResetViewButton } from '~/components/ui/reset-view-button';

type View<T extends string> = {
  value: T;
  label: string;
  count?: number;
};

type ViewsToolbarProps<T extends string> = {
  views: readonly View<T>[];
  view: T;
  onViewChange: (view: T) => void;
  searchLabel: string;
  search: string;
  onSearchChange: (search: string) => void;
  isViewChanged: boolean;
  onReset: () => void;
  actions?: ReactNode;
};

export function ViewsToolbar<T extends string>(props: ViewsToolbarProps<T>) {
  const {
    views,
    view,
    onViewChange,
    searchLabel,
    search,
    onSearchChange,
    isViewChanged,
    onReset,
    actions,
  } = props;

  const current = views.find((item) => item.value === view) ?? views[0]!;

  return (
    <div className="flex h-9 w-full items-center gap-0.5 rounded-lg bg-surface-default p-1 shadow-filter-bar ring-1 ring-border-elevated ring-inset">
      <Menu.Root>
        <Menu.Trigger
          aria-label={`View: ${current.label}`}
          className="flex h-7 shrink-0 items-center gap-1 rounded-sm pr-1.5 pl-2.5 text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:bg-surface-fill"
        >
          {current.label}
          <Icon name="chevrons-updown" className="size-3.5" />
        </Menu.Trigger>
        <Menu.Portal>
          <Menu.Positioner align="start" sideOffset={6}>
            <Menu.Popup>
              <Menu.Group>
                <Menu.GroupLabel>Views</Menu.GroupLabel>
                <Menu.RadioGroup
                  value={current.value}
                  onValueChange={(value) => onViewChange(value as T)}
                >
                  {views.map((item) => (
                    <Menu.RadioItem key={item.value} value={item.value}>
                      {item.label}
                      {item.count !== undefined && (
                        <span className="ml-auto text-caption font-normal text-text-tertiary">{item.count}</span>
                      )}
                    </Menu.RadioItem>
                  ))}
                </Menu.RadioGroup>
              </Menu.Group>
            </Menu.Popup>
          </Menu.Positioner>
        </Menu.Portal>
      </Menu.Root>
      <label className="flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-sm pr-2.5 pl-2">
        <Icon name="search" className="shrink-0 text-text-tertiary" />
        <input
          type="search"
          aria-label={searchLabel}
          placeholder={searchLabel}
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          className="min-w-0 flex-1 bg-transparent text-body text-text-primary outline-none placeholder:text-text-tertiary [&::-webkit-search-cancel-button]:hidden"
        />
      </label>
      {isViewChanged && <ResetViewButton onReset={onReset} />}
      {actions}
    </div>
  );
}
