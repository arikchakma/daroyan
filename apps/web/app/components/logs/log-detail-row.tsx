import type { ReactNode } from 'react';

import { cn } from '~/utils/classname';

type LogDetailRowProps = {
  label: string;
  mono?: boolean;
  children: ReactNode;
};

export function LogDetailRow(props: LogDetailRowProps) {
  const { label, mono, children } = props;

  return (
    <div className="flex gap-3">
      <dt className="w-24 shrink-0 truncate text-text-secondary">{label}</dt>
      <dd className={cn('min-w-0 flex-1 wrap-anywhere', mono && 'font-mono text-code')}>{children}</dd>
    </div>
  );
}
