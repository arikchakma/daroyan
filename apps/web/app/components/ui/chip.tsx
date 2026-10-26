import type { ReactNode } from 'react';

import { cn } from '~/utils/classname';

type ChipProps = {
  className?: string;
  children: ReactNode;
};

export function Chip(props: ChipProps) {
  const { className, children } = props;

  return (
    <span
      className={cn(
        'flex h-5.5 max-w-full min-w-0 shrink-0 items-center truncate rounded-xs bg-surface-fill px-1.75 text-caption font-normal text-text-secondary',
        className
      )}
    >
      <span className="truncate">{children}</span>
    </span>
  );
}
