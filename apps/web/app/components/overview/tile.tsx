import type { ReactNode } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';

type TileProps = {
  title: string;
  href: string;
  linkLabel: string;
  value: string;
  caption?: string;
  chart: ReactNode;
  healthy: ReactNode;
  children: ReactNode[] | ReactNode;
};

export function Tile(props: TileProps) {
  const { title, href, linkLabel, value, caption, chart, healthy, children } = props;

  const rows = Array.isArray(children) ? children.filter(Boolean) : children ? [children] : [];

  return (
    <section className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default pb-1 shadow-card">
      <div className="flex flex-col gap-3 p-4">
        <div className="flex items-center gap-2">
          <h2 className="flex-1 text-title-section font-medium">{title}</h2>
          <Link
            to={href}
            className="text-body text-text-secondary outline-none hover:text-text-primary focus-visible:underline"
          >
            {linkLabel}
          </Link>
        </div>
        <p className="flex items-baseline gap-1.5 whitespace-nowrap">
          <span className="text-body font-semibold">{value}</span>
          {caption && <span className="truncate text-caption font-normal text-text-tertiary">{caption}</span>}
        </p>
        {chart}
      </div>
      <div className="flex flex-col [&>*]:border-t [&>*]:border-border-row">
        {rows.length > 0 ? (
          rows
        ) : (
          <div className="flex items-center gap-2.5 px-4 py-2.75">
            <Badge tone="success">
              <Icon name="check" />
              Healthy
            </Badge>
            <span className="truncate text-body text-text-secondary">{healthy}</span>
          </div>
        )}
      </div>
    </section>
  );
}
