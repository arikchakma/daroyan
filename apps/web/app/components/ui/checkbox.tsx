import { Checkbox as CheckboxPrimitive } from '@base-ui/react/checkbox';

import { Icon } from '~/components/icons/icon';
import { cn } from '~/utils/classname';

type CheckboxProps = CheckboxPrimitive.Root.Props;

export function Checkbox(props: CheckboxProps) {
  const { className, ...rest } = props;

  return (
    <CheckboxPrimitive.Root
      className={cn(
        'peer group flex size-4 shrink-0 items-center justify-center rounded-xs border border-border-strong bg-surface-default shadow-button-secondary outline-none focus-visible:ring-2 focus-visible:ring-text-link/40 data-checked:border-transparent data-checked:bg-button-primary data-checked:shadow-none data-disabled:cursor-not-allowed data-disabled:opacity-50 data-indeterminate:border-transparent data-indeterminate:bg-button-primary data-indeterminate:shadow-none',
        className
      )}
      {...rest}
    >
      <CheckboxPrimitive.Indicator className="flex items-center justify-center text-text-on-primary">
        <Icon name="check" className="size-3 group-data-indeterminate:hidden" />
        <Icon
          name="minus"
          className="hidden size-3 group-data-indeterminate:block"
        />
      </CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
  );
}
