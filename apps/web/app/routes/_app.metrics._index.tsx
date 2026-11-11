import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { MetricCatalog } from '~/components/metrics/metric-catalog';
import { MetricView } from '~/components/metrics/metric-view';
import { Button } from '~/components/ui/button';
import { EmptyState } from '~/components/ui/empty-state';
import { PageBar } from '~/components/ui/page-bar';
import { RadioMenu } from '~/components/ui/radio-menu';
import { metricQueries } from '~/queries/metrics';
import { serverQueries } from '~/queries/servers';
import { DAY_MS, HOUR_MS } from '~/utils/format';
import { readMetricFilters } from '~/utils/metrics';
import type { MetricAggregation } from '~/utils/metrics';

const RANGES = [
  { value: '1h', label: 'Last Hour', ms: HOUR_MS },
  { value: '6h', label: 'Last 6 Hours', ms: 6 * HOUR_MS },
  { value: '24h', label: 'Last 24 Hours', ms: DAY_MS },
  { value: '7d', label: 'Last 7 Days', ms: 7 * DAY_MS },
  { value: '30d', label: 'Last 30 Days', ms: 30 * DAY_MS },
] as const;

export function meta() {
  return [{ title: 'Metrics · Daroyan' }];
}

export default function MetricsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const range = RANGES.find((item) => item.value === searchParams.get('range')) ?? RANGES[0];
  const serverId = searchParams.get('server') ?? '';
  const service = searchParams.get('service') ?? '';

  const { data: metrics } = useQuery(metricQueries.list());
  const { data: servers } = useQuery(serverQueries.list());
  const server = servers?.find((item) => item.id === serverId);
  const services = [...new Set((metrics ?? []).flatMap((metric) => metric.services))].sort();
  const name = searchParams.get('metric') ?? metrics?.[0]?.name;
  const metric = metrics?.find((item) => item.name === name);

  const setParams = (changes: Record<string, string | null>) =>
    setSearchParams(
      (params) => {
        for (const [key, value] of Object.entries(changes)) {
          if (value) {
            params.set(key, value);
          } else {
            params.delete(key);
          }
        }
        return params;
      },
      { replace: true }
    );

  if (metrics && metrics.length === 0) {
    return (
      <div className="flex min-h-full flex-col bg-surface-default">
        <PageBar icon="chart-line" title="Metrics" />
        <EmptyState
          icon="chart-line"
          marks={['check', 'clock', 'check']}
          title="No metrics yet"
          description="Point any OpenTelemetry SDK at Daroyan to chart queue depth, request rates and anything else your app counts."
        >
          <Button nativeButton={false} render={<Link to="/traces/setup" />} className="w-fit">
            <Icon name="settings" />
            Set Up Metrics
          </Button>
        </EmptyState>
      </div>
    );
  }

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="chart-line" title="Metrics">
        {servers && servers.length > 0 && (
          <RadioMenu
            icon="server"
            label={server?.name ?? 'All Servers'}
            value={serverId}
            options={[{ value: '', label: 'All Servers' }, ...servers.map((item) => ({ value: item.id, label: item.name }))]}
            onChange={(value) => setParams({ server: value || null })}
          />
        )}
        {services.length > 0 && (
          <RadioMenu
            icon="layers"
            label={service || 'All Services'}
            value={service}
            options={[{ value: '', label: 'All Services' }, ...services.map((item) => ({ value: item, label: item }))]}
            onChange={(value) => setParams({ service: value || null })}
          />
        )}
        <RadioMenu
          icon="calendar"
          label={range.label}
          value={range.value}
          options={[...RANGES]}
          onChange={(value) => setParams({ range: value === '1h' ? null : value })}
        />
        <Button nativeButton={false} variant="secondary" render={<Link to="/traces/setup" />} className="w-fit">
          <Icon name="terminal" />
          Set Up
        </Button>
      </PageBar>

      <div className="flex flex-1 gap-4 px-3 pb-3 max-lg:flex-col">
        <MetricCatalog
          metrics={metrics}
          selected={metric?.name}
          onSelect={(next) => setParams({ metric: next, agg: null, group: null, filter: null })}
        />
        {metric && (
          <MetricView
            key={metric.name}
            metric={metric}
            rangeMs={range.ms}
            service={service || undefined}
            serverId={serverId || undefined}
            aggregation={(searchParams.get('agg') as MetricAggregation | null) ?? undefined}
            groupBy={searchParams.get('group') || undefined}
            filters={readMetricFilters(searchParams.get('filter') ?? '')}
            onChange={setParams}
          />
        )}
      </div>
    </div>
  );
}
