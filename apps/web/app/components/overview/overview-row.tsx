import { cva } from 'class-variance-authority';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';

const rowIcon = cva('shrink-0', {
  variants: {
    tone: {
      critical: 'text-tone-critical-fg',
      warning: 'text-tone-warning-fg',
      neutral: 'text-text-secondary',
    },
  },
});

type OverviewRowProps = {
  href: string;
  icon: IconName;
  tone: 'critical' | 'warning' | 'neutral';
  title: string;
  detail: string;
  date?: string;
};

export function OverviewRow(props: OverviewRowProps) {
  const { href, icon, tone, title, detail, date } = props;

  return (
    <Link
      to={href}
      className="flex items-center gap-3 px-4 py-3 outline-none hover:bg-surface-subtle focus-visible:bg-surface-subtle"
    >
      <Icon name={icon} className={rowIcon({ tone })} />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5 text-body">
        <span className="truncate font-medium">{title}</span>
        <span className="truncate text-text-secondary">{detail}</span>
      </div>
      {date && <span className="shrink-0 text-caption font-normal text-text-secondary">{date}</span>}
      <Icon name="chevron-right" className="text-text-tertiary" />
    </Link>
  );
}
