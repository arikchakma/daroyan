import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { Button } from '~/components/ui/button';
import { Menu } from '~/components/ui/menu';

type RadioMenuProps = {
  icon: IconName;
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
  size?: 'default' | 'xs';
};

export function RadioMenu(props: RadioMenuProps) {
  const { icon, label, value, options, onChange, size } = props;

  return (
    <Menu.Root>
      <Menu.Trigger render={<Button variant="secondary" size={size} className="w-fit" />}>
        <Icon name={icon} />
        {label}
        <Icon name="chevron-down" />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner align="end">
          <Menu.Popup className="max-h-80 min-w-40 overflow-y-auto">
            <Menu.RadioGroup value={value} onValueChange={(next: string) => onChange(next)}>
              {options.map((option) => (
                <Menu.RadioItem key={option.value} value={option.value}>
                  {option.label}
                </Menu.RadioItem>
              ))}
            </Menu.RadioGroup>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
