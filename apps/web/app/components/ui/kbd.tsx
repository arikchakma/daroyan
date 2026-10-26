import type { ComponentProps } from 'react';

import { cn } from '~/utils/classname';

type KbdProps = ComponentProps<'kbd'>;

export function Kbd(props: KbdProps) {
  const { className, ...rest } = props;

  return (
    <kbd
      className={cn(
        'flex h-5 min-w-5 shrink-0 items-center justify-center rounded-[5px] border border-b-2 border-border-ring bg-surface-default px-1.25 font-sans text-axis font-medium text-text-secondary [&_svg]:size-3',
        className
      )}
      {...rest}
    />
  );
}
