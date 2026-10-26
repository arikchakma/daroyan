import type { ReactNode } from 'react';

import { Icon } from '~/components/icons/icon';
import { Menu } from '~/components/ui/menu';

type BulkBarProps = {
  label: string;
  countLabel: string;
  pageCount: number;
  totalLabel?: string | null;
  onSelectPage: () => void;
  onSelectAll?: () => void;
  onClear: () => void;
  children?: ReactNode;
};

export function BulkBar(props: BulkBarProps) {
  const {
    label,
    countLabel,
    pageCount,
    totalLabel,
    onSelectPage,
    onSelectAll,
    onClear,
    children,
  } = props;

  const canSelectAll = !!totalLabel && !!onSelectAll;

  return (
    <div
      role="toolbar"
      aria-label={`Selected ${label}`}
      className="flex h-7.5 w-full min-w-0 items-center gap-1.5 font-normal text-text-primary"
    >
      <Menu.Root>
        <Menu.Trigger className="flex h-6 shrink-0 items-center gap-0.5 rounded-full pr-1 pl-2 text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:bg-surface-fill">
          {countLabel} selected
          <Icon name="chevron-down" />
        </Menu.Trigger>
        <Menu.Portal>
          <Menu.Positioner align="start" sideOffset={4}>
            <Menu.Popup>
              <Menu.Item onClick={onSelectPage}>
                Select All {pageCount.toLocaleString('en-US')} on Page
              </Menu.Item>
              {canSelectAll && (
                <Menu.Item onClick={onSelectAll}>
                  Select All {totalLabel} {label}
                </Menu.Item>
              )}
              <Menu.Item onClick={onClear}>Unselect All</Menu.Item>
            </Menu.Popup>
          </Menu.Positioner>
        </Menu.Portal>
      </Menu.Root>
      {children}
      {canSelectAll && (
        <button
          type="button"
          onClick={onSelectAll}
          className="ml-auto h-6 shrink-0 rounded-full px-2 text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40"
        >
          Select All {totalLabel} {label}
        </button>
      )}
    </div>
  );
}
