import type { ReactNode } from 'react';
import { Link } from 'react-router';

import { cn } from '~/utils/classname';

type TileRowProps = {
  href: string;
  badge: ReactNode;
  label: string;
  meta: string;
  isMono?: boolean;
  more?: number;
};

export function TileRow(props: TileRowProps) {
  const { href, badge, label, meta, isMono, more = 0 } = props;

  return (
    <Link
      to={href}
      className="flex items-center gap-2.5 px-4 py-2.75 outline-none hover:bg-surface-subtle focus-visible:bg-surface-subtle"
    >
      {badge}
      <span className="flex min-w-0 flex-1 items-baseline gap-1.5">
        <span className={cn('truncate text-body', isMono && 'font-mono text-code')}>{label}</span>
        {more > 0 && <span className="shrink-0 text-caption font-normal text-text-tertiary">+{more} more</span>}
      </span>
      <span className="shrink-0 text-caption font-normal text-text-tertiary">{meta}</span>
    </Link>
  );
}
