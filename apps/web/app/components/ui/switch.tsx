import { Switch as SwitchPrimitive } from '@base-ui/react/switch';

import { cn } from '~/utils/classname';

type SwitchRootProps = SwitchPrimitive.Root.Props;

function SwitchRoot(props: SwitchRootProps) {
  const { className, ...rest } = props;

  return (
    <SwitchPrimitive.Root
      className={cn(
        'peer inline-flex h-5 w-8 shrink-0 items-center rounded-full bg-surface-fill-strong p-0.5 transition-colors outline-none focus-visible:ring-2 focus-visible:ring-text-link/40 data-checked:bg-button-primary data-disabled:cursor-not-allowed data-disabled:opacity-50',
        className
      )}
      {...rest}
    />
  );
}

type SwitchThumbProps = SwitchPrimitive.Thumb.Props;

function SwitchThumb(props: SwitchThumbProps) {
  const { className, ...rest } = props;

  return (
    <SwitchPrimitive.Thumb
      className={cn(
        'pointer-events-none block size-4 rounded-full bg-surface-default shadow-button-secondary ring-1 ring-border-elevated transition-transform ring-inset data-checked:translate-x-3 data-unchecked:translate-x-0',
        className
      )}
      {...rest}
    />
  );
}

export const Switch = {
  Root: SwitchRoot,
  Thumb: SwitchThumb,
};
