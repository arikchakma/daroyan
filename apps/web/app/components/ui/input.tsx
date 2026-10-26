import { Input as InputPrimitive } from '@base-ui/react/input';

import { cn } from '~/utils/classname';

export type InputProps = InputPrimitive.Props;

export function Input(props: InputProps) {
  const { className, ...rest } = props;

  return (
    <InputPrimitive
      className={cn(
        'block h-9 w-full rounded-lg border border-border-ring bg-surface-default pr-2.5 pl-3 text-body text-text-primary outline-none placeholder:text-text-tertiary focus-visible:border-border-strong disabled:opacity-60 aria-invalid:border-tone-critical-fg data-disabled:opacity-60',
        className
      )}
      {...rest}
    />
  );
}
