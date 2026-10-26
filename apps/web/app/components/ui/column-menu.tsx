import type { ColumnVisibilityState } from '@tanstack/react-table';

import { Icon } from '~/components/icons/icon';
import { Menu } from '~/components/ui/menu';

type ColumnMenuProps = {
  labels: Record<string, string>;
  hidden: ColumnVisibilityState;
  onChange: (hidden: ColumnVisibilityState) => void;
};

export function ColumnMenu(props: ColumnMenuProps) {
  const { labels, hidden, onChange } = props;

  return (
    <Menu.Root>
      <Menu.Trigger
        aria-label="Columns"
        title="Columns"
        className="flex size-7 shrink-0 items-center justify-center rounded-sm text-text-secondary outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:bg-surface-fill"
      >
        <Icon name="columns" />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner align="end" sideOffset={6}>
          <Menu.Popup>
            <Menu.Group>
              <Menu.GroupLabel>Columns</Menu.GroupLabel>
              {Object.entries(labels).map(([id, label]) => (
                <Menu.CheckboxItem
                  key={id}
                  checked={hidden[id] !== false}
                  onCheckedChange={(checked: boolean) => onChange({ ...hidden, [id]: checked })}
                >
                  {label}
                </Menu.CheckboxItem>
              ))}
            </Menu.Group>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
