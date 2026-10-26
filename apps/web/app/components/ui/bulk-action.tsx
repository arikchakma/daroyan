import type { ComponentPropsWithRef } from 'react';

import { cn } from '~/utils/classname';

type BulkActionProps = ComponentPropsWithRef<'button'>;

export function BulkAction(props: BulkActionProps) {
  const { className, ...rest } = props;

  return (
    <button
      type="button"
      className={cn(
        'flex h-6 shrink-0 items-center justify-center rounded-full bg-tone-neutral-bg px-2.5 text-body font-medium text-text-primary outline-none hover:bg-surface-pill focus-visible:ring-2 focus-visible:ring-text-link/40 disabled:opacity-60 data-popup-open:bg-surface-pill',
        className
      )}
      {...rest}
    />
  );
}
