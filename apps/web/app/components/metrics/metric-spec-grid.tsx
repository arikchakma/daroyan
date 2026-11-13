import { useQueries, useQuery } from '@tanstack/react-query';

import { Icon } from '~/components/icons/icon';
import { Spec } from '~/components/servers/spec';
import type { SpecProps } from '~/components/servers/spec';
import { metricQueries } from '~/queries/metrics';
import { formatClock, formatCount, formatRelative, pluralize } from '~/utils/format';
import { formatMetricValue, getSpecReads } from '~/utils/metrics';
import type { MetricSummary } from '~/utils/metrics';

type MetricSpecGridProps = {
  metric: MetricSummary;
};

export function MetricSpecGrid(props: MetricSpecGridProps) {
  const { metric } = props;

  const reads = getSpecReads(metric);
  const results = useQueries({
    queries: reads.map((read) =>
      metricQueries.query({
        name: metric.name,
        rangeMs: read.windowMs,
        agg: read.aggregation,
        step_ms: read.windowMs,
      })
    ),
  });
  const { data: attributes = [] } = useQuery(metricQueries.attributes(metric.name));

  const specs: SpecProps[] = [
    ...reads.map((read, index) => {
      const value = results[index]?.data?.series[0]?.values.at(-1);
      return {
        label: read.label,
        value: value === undefined || value === null ? '—' : formatMetricValue(value, metric.unit, read.aggregation),
        detail: read.detail,
      };
    }),
    { label: 'Series', value: formatCount(metric.series), detail: pluralize(metric.services.length, 'service') },
    { label: 'Last Point', value: formatClock(metric.last_seen_at), detail: formatRelative(metric.last_seen_at) },
  ];

  return (
    <div className="min-w-0 flex-1 overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card">
      <div className="grid grid-cols-2 gap-px bg-border-row md:grid-cols-3">
        {specs.map((spec) => (
          <Spec key={`${spec.label}-${spec.detail}`} {...spec} />
        ))}
      </div>
      <details className="group border-t border-border-row">
        <summary className="flex cursor-pointer list-none items-center gap-2 px-4 py-2.5 text-caption font-medium text-text-secondary select-none">
          <Icon name="info-fill" className="text-icon-secondary" />
          Attributes
          <Icon name="chevron-down" className="ml-auto text-icon-secondary transition-transform group-open:rotate-180" />
        </summary>
        {attributes.length === 0 ? (
          <p className="border-t border-border-row px-4 py-3 text-body text-text-secondary">This metric has no attributes.</p>
        ) : (
          <div className="grid grid-cols-2 gap-px border-t border-border-row bg-border-row md:grid-cols-3">
            {attributes.map((attribute) => (
              <div key={attribute.key} className="flex min-w-0 flex-col gap-1 bg-surface-default px-4 py-3">
                <span className="truncate text-caption font-medium text-text-secondary">{attribute.key}</span>
                <span className="min-w-0 truncate font-mono text-code text-text-primary">
                  {attribute.values.map((item) => item.value).join(', ')}
                </span>
              </div>
            ))}
          </div>
        )}
      </details>
    </div>
  );
}
