import type { ReactNode } from 'react';
import { Link } from 'react-router';

import { cn } from '~/utils/classname';

type MetricSideRowProps = {
  to: string;
  label: ReactNode;
  detail: string;
  isCritical?: boolean;
};

export function MetricSideRow(props: MetricSideRowProps) {
  const { to, label, detail, isCritical } = props;

  return (
    <li className="border-t border-border-row first:border-t-0">
      <Link
        to={to}
        className="flex h-8.5 items-center gap-3 px-4 outline-none hover:bg-surface-fill focus-visible:bg-surface-fill"
      >
        <span className={cn('min-w-0 flex-1 truncate text-body text-text-primary', isCritical && 'text-tone-critical-fg')}>
          {label}
        </span>
        <span className="shrink-0 font-mono text-code-small text-text-tertiary">{detail}</span>
      </Link>
    </li>
  );
}
