import type { ReactNode } from 'react';

import { cn } from '~/utils/classname';

type StatProps = {
  label: string;
  value: string;
  detail?: string;
  valueClassName?: string;
};

export function Stat(props: StatProps) {
  const { label, value, detail, valueClassName } = props;

  return (
    <div className="flex min-w-0 flex-col gap-1 bg-surface-default px-4 py-3.5">
      <span className="text-caption font-medium text-text-secondary">{label}</span>
      <div className="flex min-w-0 flex-wrap items-baseline gap-x-1.5 whitespace-nowrap">
        <span className={cn('max-w-full truncate text-body font-semibold text-text-primary', valueClassName)}>{value}</span>
        {detail && <span className="max-w-full truncate text-caption font-normal text-text-tertiary">{detail}</span>}
      </div>
    </div>
  );
}

const STRIP_COLUMNS = {
  4: 'grid-cols-2 @lg:grid-cols-4',
  6: 'grid-cols-3 @3xl:grid-cols-6',
};

type StatStripProps = {
  columns: keyof typeof STRIP_COLUMNS;
  children: ReactNode;
};

export function StatStrip(props: StatStripProps) {
  const { columns, children } = props;

  return (
    <div className="@container">
      <div
        className={cn(
          'grid gap-px overflow-hidden rounded-2xl border border-border-ring bg-border-row shadow-card',
          STRIP_COLUMNS[columns]
        )}
      >
        {children}
      </div>
    </div>
  );
}

type RailSectionProps = {
  title: string;
  caption?: string;
  isLast?: boolean;
  children: ReactNode;
};

export function RailSection(props: RailSectionProps) {
  const { title, caption, isLast, children } = props;

  return (
    <div className={isLast ? 'flex flex-col gap-2.5' : 'flex flex-col gap-2.5 border-b border-border-row pb-4'}>
      <div className="flex items-baseline gap-3">
        <h2 className="flex-1 text-body font-medium">{title}</h2>
        {caption && <span className="text-body text-text-tertiary">{caption}</span>}
      </div>
      {children}
    </div>
  );
}

type RailRowProps = {
  label: string;
  children: ReactNode;
};

export function RailRow(props: RailRowProps) {
  const { label, children } = props;

  return (
    <div className="flex items-start gap-3 text-body">
      <span className="flex-1 text-text-secondary">{label}</span>
      <span className="text-right text-text-primary">{children}</span>
    </div>
  );
}
