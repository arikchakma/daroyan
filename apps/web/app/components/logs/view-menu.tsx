import { Icon } from '~/components/icons/icon';
import { Menu } from '~/components/ui/menu';
import type { LogLevel } from '~/queries/logs';

export type LevelView = 'all' | LogLevel;

export const VIEWS: { value: LevelView; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'error', label: 'Errors' },
  { value: 'warn', label: 'Warnings and Errors' },
  { value: 'info', label: 'Info and Above' },
];

type ViewMenuProps = {
  view: LevelView;
  onChange: (view: LevelView) => void;
};

export function ViewMenu(props: ViewMenuProps) {
  const { view, onChange } = props;

  const current = VIEWS.find((item) => item.value === view) ?? VIEWS[0]!;

  return (
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
              <Menu.RadioGroup value={current.value} onValueChange={(value) => onChange(value as LevelView)}>
                {VIEWS.map((item) => (
                  <Menu.RadioItem key={item.value} value={item.value}>
                    {item.label}
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
