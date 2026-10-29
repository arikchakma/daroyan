import type { ReactNode } from 'react';

import { cn } from '~/utils/classname';

type FieldGridProps = {
  hint?: string;
  columns?: 2 | 3;
  children: ReactNode;
};

export function FieldGrid(props: FieldGridProps) {
  const { hint, columns = 2, children } = props;

  return (
    <div className="flex flex-col gap-1.5">
      <div className={cn('grid grid-cols-1 gap-4', columns === 3 ? 'sm:grid-cols-3' : 'sm:grid-cols-2')}>{children}</div>
      {hint && <p className="text-caption leading-4.5 text-text-secondary">{hint}</p>}
    </div>
  );
}
