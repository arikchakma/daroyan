import { useQuery } from '@tanstack/react-query';
import type { ColumnVisibilityState } from '@tanstack/react-table';
import { Link, useSearchParams } from 'react-router';

import { AlertBanners } from '~/components/alerts/alert-banners';
import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { ColumnMenu } from '~/components/ui/column-menu';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState, NoResults } from '~/components/ui/empty-state';
import { PageBar } from '~/components/ui/page-bar';
import { RadioMenu } from '~/components/ui/radio-menu';
import { ViewsToolbar } from '~/components/ui/views-toolbar';
import { METRIC_COLUMN_LABELS, useMetricsTable } from '~/hooks/use-metrics-table';
import { useStoredState } from '~/hooks/use-stored-state';
import { metricQueries } from '~/queries/metrics';
import { getKindLabel, METRIC_RANGES, METRICS_TARGET_ID, readMetricRange } from '~/utils/metrics';

const COLUMNS_KEY = 'daroyan.metrics.columns';

type KindView = 'all' | 'Histogram' | 'Counter' | 'Gauge';

const VIEWS: { value: KindView; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'Histogram', label: 'Histograms' },
  { value: 'Counter', label: 'Counters' },
  { value: 'Gauge', label: 'Gauges' },
];

export function meta() {
  return [{ title: 'Metrics · Daroyan' }];
}

export default function MetricsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const range = readMetricRange(searchParams.get('range'));
  const view = VIEWS.find((item) => item.value === searchParams.get('view'))?.value ?? 'all';
  const search = searchParams.get('q') ?? '';

  const [hiddenColumns, setHiddenColumns] = useStoredState<ColumnVisibilityState>(COLUMNS_KEY, {});

  const { data: metrics, isLoading } = useQuery(metricQueries.list(range.ms));
  const text = search.trim().toLowerCase();
  const shown = (metrics ?? []).filter(
    (metric) =>
      (view === 'all' || getKindLabel(metric) === view) &&
      (metric.name.toLowerCase().includes(text) ||
        metric.services.some((service) => service.toLowerCase().includes(text)))
  );
  const table = useMetricsTable({ metrics: shown, columnVisibility: hiddenColumns });

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

  if (metrics && metrics.length === 0 && !searchParams.get('range')) {
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
        <RadioMenu
          icon="calendar"
          label={range.label}
          value={range.value}
          options={[...METRIC_RANGES]}
          onChange={(value) => setParams({ range: value === '1h' ? null : value })}
        />
      </PageBar>

      <div className="flex flex-1 flex-col gap-2 px-3 pb-3">
        <AlertBanners serverId={METRICS_TARGET_ID} />
        <ViewsToolbar
          views={VIEWS}
          view={view}
          onViewChange={(next) => setParams({ view: next === 'all' ? null : next })}
          searchLabel="Search by metric or service"
          search={search}
          onSearchChange={(next) => setParams({ q: next || null })}
          isViewChanged={view !== 'all' || !!search}
          onReset={() => setSearchParams({}, { replace: true })}
          actions={<ColumnMenu labels={METRIC_COLUMN_LABELS} hidden={hiddenColumns} onChange={setHiddenColumns} />}
        />
        <DataTable table={table} label="Metrics" size="sm" isLoading={isLoading} />
        {metrics && shown.length === 0 && (
          <NoResults title="No metrics found" onReset={() => setSearchParams({}, { replace: true })} />
        )}
      </div>
    </div>
  );
}
