import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { Menu } from '~/components/ui/menu';
import type { MetricAttribute } from '~/utils/metrics';

type MetricFilterMenuProps = {
  attributes?: MetricAttribute[];
  usedKeys: string[];
  onAdd: (key: string, value: string) => void;
};

export function MetricFilterMenu(props: MetricFilterMenuProps) {
  const { attributes, usedKeys, onAdd } = props;

  const available = (attributes ?? []).filter((attribute) => !usedKeys.includes(attribute.key));

  return (
    <Menu.Root>
      <Menu.Trigger
        render={<Button variant="secondary" size="xs" disabled={available.length === 0} />}
      >
        <Icon name="plus" />
        Filter
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner align="start">
          <Menu.Popup className="min-w-44">
            {available.map((attribute) => (
              <Menu.SubmenuRoot key={attribute.key}>
                <Menu.SubmenuTrigger>
                  <Icon name="tag" />
                  <span className="truncate font-mono text-code-small">{attribute.key}</span>
                </Menu.SubmenuTrigger>
                <Menu.Portal>
                  <Menu.Positioner sideOffset={4}>
                    <Menu.Popup className="max-h-80 min-w-44 overflow-y-auto">
                      {attribute.values.map((item) => (
                        <Menu.Item key={item.value} onClick={() => onAdd(attribute.key, item.value)}>
                          <span className="flex-1 truncate">{item.value}</span>
                          <span className="pl-4 text-caption text-text-tertiary">{item.series}</span>
                        </Menu.Item>
                      ))}
                    </Menu.Popup>
                  </Menu.Positioner>
                </Menu.Portal>
              </Menu.SubmenuRoot>
            ))}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
