import type { ReactNode } from 'react';
import { Link } from 'react-router';

type MetricSideCardProps = {
  title: string;
  linkLabel: string;
  linkTo: string;
  empty?: string;
  children?: ReactNode;
};

export function MetricSideCard(props: MetricSideCardProps) {
  const { title, linkLabel, linkTo, empty, children } = props;

  return (
    <div className="flex w-full shrink-0 flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card lg:w-85">
      <header className="flex items-center px-4 pt-2.5 pb-1">
        <h2 className="flex-1 text-caption font-medium text-text-primary">{title}</h2>
        <Link to={linkTo} className="rounded-xs text-body text-text-primary outline-none hover:underline focus-visible:underline">
          {linkLabel}
        </Link>
      </header>
      {empty ? <p className="flex h-8.5 items-center px-4 text-body text-text-secondary">{empty}</p> : <ul>{children}</ul>}
    </div>
  );
}
