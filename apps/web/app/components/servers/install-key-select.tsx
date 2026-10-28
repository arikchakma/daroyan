import { Select } from '~/components/ui/select';

type Item = { value: string; label: string };

const NONE = 'none';

export const EXPIRES_ITEMS: Item[] = [
  { value: '24', label: '24 hours' },
  { value: '168', label: '7 days' },
  { value: '720', label: '30 days' },
  { value: '2160', label: '90 days' },
  { value: NONE, label: 'Never' },
];

export const MAX_USES_ITEMS: Item[] = [
  ...[1, 5, 25, 100].map((count) => ({
    value: String(count),
    label: count === 1 ? '1 server' : `${count} servers`,
  })),
  { value: NONE, label: 'Unlimited' },
];

function toLimit(value: number | null) {
  return value === null ? NONE : String(value);
}

type InstallKeySelectProps = {
  label: string;
  items: Item[];
  value: number | null;
  onValueChange: (value: number | null) => void;
  disabled?: boolean;
  className?: string;
};

export function InstallKeySelect(props: InstallKeySelectProps) {
  const { label, items, value, onValueChange, disabled, className } = props;

  return (
    <Select.Root
      items={items}
      value={toLimit(value)}
      onValueChange={(next) => next && onValueChange(next === NONE ? null : Number(next))}
      disabled={disabled}
    >
      <Select.Trigger aria-label={label} className={className}>
        <Select.Value />
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner>
          <Select.Popup>
            {items.map((item) => (
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
