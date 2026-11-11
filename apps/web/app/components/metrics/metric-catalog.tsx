import { useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { Input } from '~/components/ui/input';
import { Skeleton } from '~/components/ui/skeleton';
import { cn } from '~/utils/classname';
import { getKindLabel } from '~/utils/metrics';
import type { MetricSummary } from '~/utils/metrics';

type MetricCatalogProps = {
  metrics?: MetricSummary[];
  selected?: string;
  onSelect: (name: string) => void;
};

export function MetricCatalog(props: MetricCatalogProps) {
  const { metrics, selected, onSelect } = props;

  const [search, setSearch] = useState('');
  const text = search.trim().toLowerCase();
  const shown = (metrics ?? []).filter(
    (metric) =>
      metric.name.toLowerCase().includes(text) ||
      metric.services.some((service) => service.toLowerCase().includes(text))
  );

  return (
    <aside
      aria-label="Metrics"
      className="flex w-72 shrink-0 flex-col gap-2 rounded-2xl border border-border-ring bg-surface-default p-2 shadow-card max-lg:w-full"
    >
      <div className="relative">
        <Icon name="search" className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-icon-secondary" />
        <Input
          aria-label="Search metrics"
          placeholder="Search metrics or services"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          className="h-8 pl-8"
        />
      </div>

      <ul className="flex max-h-[calc(100dvh-10rem)] flex-col gap-0.5 overflow-y-auto max-lg:max-h-72">
        {!metrics &&
          Array.from({ length: 6 }, (_, index) => (
            <li key={index} className="flex flex-col gap-1.5 px-2.5 py-2">
              <Skeleton className="w-3/4" />
              <Skeleton className="w-1/2" />
            </li>
          ))}
        {shown.map((metric) => (
          <li key={metric.name}>
            <button
              type="button"
              aria-current={metric.name === selected ? 'true' : undefined}
              onClick={() => onSelect(metric.name)}
              className={cn(
                'flex w-full flex-col gap-0.5 rounded-lg px-2.5 py-1.75 text-left outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40',
                metric.name === selected && 'bg-surface-fill'
              )}
            >
              <span className="truncate font-mono text-code-small text-text-primary">{metric.name}</span>
              <span className="truncate text-caption text-text-tertiary">
                {[getKindLabel(metric), metric.unit, metric.services.join(', ')].filter(Boolean).join(' · ')}
              </span>
            </button>
          </li>
        ))}
        {metrics && shown.length === 0 && (
          <li className="px-2.5 py-2 text-caption text-text-tertiary">No metrics match “{search}”</li>
        )}
      </ul>
    </aside>
  );
}
