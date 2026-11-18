import type { ColumnVisibilityState } from '@tanstack/react-table';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { Menu } from '~/components/ui/menu';

type ViewOptionsMenuProps = {
  labels: Record<string, string>;
  hidden: ColumnVisibilityState;
  onHiddenChange: (hidden: ColumnVisibilityState) => void;
  isWrapped: boolean;
  onWrappedChange: (isWrapped: boolean) => void;
  pageSizes: number[];
  pageSize: number;
  onPageSizeChange: (pageSize: number) => void;
};

export function ViewOptionsMenu(props: ViewOptionsMenuProps) {
  const { labels, hidden, onHiddenChange, isWrapped, onWrappedChange, pageSizes, pageSize, onPageSizeChange } = props;

  return (
    <Menu.Root>
      <Menu.Trigger render={<Button variant="secondary" className="w-fit data-popup-open:bg-surface-fill" />}>
        <Icon name="settings-2" />
        View
        <Icon name="chevron-down" />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner align="end" sideOffset={6}>
          <Menu.Popup className="min-w-48">
            <Menu.Group>
              <Menu.GroupLabel>Columns</Menu.GroupLabel>
              {Object.entries(labels).map(([id, label]) => (
                <Menu.CheckboxItem
                  key={id}
                  checked={hidden[id] !== false}
                  onCheckedChange={(checked: boolean) => onHiddenChange({ ...hidden, [id]: checked })}
                >
                  {label}
                </Menu.CheckboxItem>
              ))}
            </Menu.Group>
            <Menu.Separator />
            <Menu.CheckboxItem checked={isWrapped} onCheckedChange={(checked: boolean) => onWrappedChange(checked)}>
              Wrap Lines
            </Menu.CheckboxItem>
            <Menu.Separator />
            <Menu.Group>
              <Menu.GroupLabel>Lines per Page</Menu.GroupLabel>
              <Menu.RadioGroup value={pageSize} onValueChange={(value: number) => onPageSizeChange(value)}>
                {pageSizes.map((size) => (
                  <Menu.RadioItem key={size} value={size}>
                    {size}
                  </Menu.RadioItem>
                ))}
              </Menu.RadioGroup>
            </Menu.Group>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
