import type { ReactNode } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { cn } from '~/utils/classname';

type MetricSideRowProps = {
  to: string;
  label: ReactNode;
  detail: string;
  isFiring?: boolean;
};

export function MetricSideRow(props: MetricSideRowProps) {
  const { to, label, detail, isFiring } = props;

  return (
    <li className="border-t border-border-row first:border-t-0">
      <Link
        to={to}
        className="flex h-8.5 items-center gap-3 px-4 outline-none hover:bg-surface-fill focus-visible:bg-surface-fill"
      >
        {isFiring && <Icon name="triangle-alert" className="size-3.5 shrink-0 text-tone-critical-fg" />}
        <span className={cn('min-w-0 flex-1 truncate text-body text-text-primary', isFiring && 'text-tone-critical-fg')}>
          {label}
          {isFiring && <span className="sr-only"> (firing)</span>}
        </span>
        <span className="shrink-0 font-mono text-code-small text-text-tertiary">{detail}</span>
      </Link>
    </li>
  );
}
