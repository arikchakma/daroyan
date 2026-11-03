import { useSyncExternalStore } from 'react';

import { Select } from '~/components/ui/select';
import { getStoredTheme, setTheme, subscribeToTheme } from '~/utils/theme';
import type { Theme } from '~/utils/theme';

const THEME_ITEMS = [
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
  { value: 'system', label: 'System' },
];

type ThemeSelectProps = {
  labelId: string;
};

export function ThemeSelect(props: ThemeSelectProps) {
  const { labelId } = props;

  const theme = useSyncExternalStore(subscribeToTheme, getStoredTheme, () => 'system');

  return (
    <Select.Root items={THEME_ITEMS} value={theme} onValueChange={(next) => next && setTheme(next as Theme)}>
      <Select.Trigger aria-labelledby={labelId} className="w-32">
        <Select.Value />
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner>
          <Select.Popup>
            {THEME_ITEMS.map((item) => (
              <Select.Item key={item.value} value={item.value}>
                {item.label}
              </Select.Item>
            ))}
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}
