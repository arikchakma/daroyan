import { useQuery } from '@tanstack/react-query';
import type { ColumnVisibilityState, SortingState } from '@tanstack/react-table';
import { useState } from 'react';
import { Link, useSearchParams } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { ColumnMenu } from '~/components/ui/column-menu';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState, NoResults } from '~/components/ui/empty-state';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { Pager } from '~/components/ui/pager';
import { ViewsToolbar } from '~/components/ui/views-toolbar';
import { TRACE_COLUMN_LABELS, useTracesTable } from '~/hooks/use-traces-table';
import { listTracesOptions, traceStatusOptions } from '~/queries/traces';
import { SLOW_MS } from '~/utils/traces';

const PAGE_SIZE = 50;
const COLUMNS_KEY = 'daroyan.traces.columns';
const HOUR_MS = 3_600_000;

const RANGES = [
  { value: '1h', label: 'Last Hour', ms: HOUR_MS },
  { value: '24h', label: 'Last 24 Hours', ms: 24 * HOUR_MS },
  { value: '7d', label: 'Last 7 Days', ms: 7 * 24 * HOUR_MS },
] as const;

type StatusView = 'all' | 'error' | 'slow';

const VIEWS: { value: StatusView; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'error', label: 'Errors' },
  { value: 'slow', label: 'Slow' },
];

const SORTS = [
  { value: 'newest', label: 'Newest First', sorting: [{ id: 'started_at', desc: true }] },
  { value: 'oldest', label: 'Oldest First', sorting: [{ id: 'started_at', desc: false }] },
  { value: 'slowest', label: 'Slowest First', sorting: [{ id: 'duration_us', desc: true }] },
  { value: 'fastest', label: 'Fastest First', sorting: [{ id: 'duration_us', desc: false }] },
] satisfies { value: string; label: string; sorting: SortingState }[];

function readColumns(): ColumnVisibilityState {
  try {
    return JSON.parse(localStorage.getItem(COLUMNS_KEY) ?? '{}') as ColumnVisibilityState;
  } catch {
    return {};
  }
}

export function meta() {
  return [{ title: 'Traces · Daroyan' }];
}

export default function TracesPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const range = RANGES.find((item) => item.value === searchParams.get('range')) ?? RANGES[0];
  const view = VIEWS.find((item) => item.value === searchParams.get('view'))?.value ?? 'all';
  const sort = SORTS.find((item) => item.value === searchParams.get('sort')) ?? SORTS[0];
  const search = searchParams.get('q') ?? '';
  const page = Math.max(Number(searchParams.get('page')) || 1, 1);

  const [hiddenColumns, setHiddenColumns] = useState(readColumns);

  const { data: status } = useQuery(traceStatusOptions());
  const { data, isLoading } = useQuery(
    listTracesOptions({
      rangeMs: range.ms,
      status: view === 'error' ? 'error' : undefined,
      min_duration_ms: view === 'slow' ? SLOW_MS : undefined,
      q: search.trim() || undefined,
      offset: (page - 1) * PAGE_SIZE,
      limit: PAGE_SIZE + 1,
    })
  );
  const traces = (data ?? []).slice(0, PAGE_SIZE);
  const table = useTracesTable({ traces, sorting: sort.sorting, columnVisibility: hiddenColumns });

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
        if (!('page' in changes)) {
          params.delete('page');
        }
        return params;
      },
      { replace: true }
    );

  if (status && !status.last_span_at) {
    return (
      <div className="flex min-h-full flex-col bg-surface-default">
        <PageBar icon="layers" title="Traces" />
        <EmptyState
          icon="layers"
          marks={['check', 'clock', 'check']}
          title="No traces yet"
          description="Point any OpenTelemetry SDK at Daroyan to see slow requests and where the time goes."
        >
          <Button nativeButton={false} render={<Link to="/traces/setup" />} className="w-fit">
            <Icon name="settings" />
            Set Up Traces
          </Button>
        </EmptyState>
      </div>
    );
  }

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="layers" title="Traces">
        <Menu.Root>
          <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
            <Icon name="calendar" />
            {range.label}
            <Icon name="chevron-down" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="end">
              <Menu.Popup className="min-w-40">
                <Menu.RadioGroup
                  value={range.value}
                  onValueChange={(value: string) => setParams({ range: value === '1h' ? null : value })}
                >
                  {RANGES.map((item) => (
                    <Menu.RadioItem key={item.value} value={item.value}>
                      {item.label}
                    </Menu.RadioItem>
                  ))}
                </Menu.RadioGroup>
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.Root>
        <Button nativeButton={false} variant="secondary" render={<Link to="/traces/setup" />} className="w-fit">
          <Icon name="terminal" />
          Set Up
        </Button>
      </PageBar>

      <div className="flex flex-1 flex-col gap-2 px-3 pb-3">
        <ViewsToolbar
          views={VIEWS}
          view={view}
          onViewChange={(next) => setParams({ view: next === 'all' ? null : next })}
          searchLabel="Search by route, job or trace ID"
          search={search}
          onSearchChange={(next) => setParams({ q: next || null })}
          isViewChanged={view !== 'all' || !!search}
          onReset={() => setSearchParams({}, { replace: true })}
          actions={
            <>
              <ColumnMenu
                labels={TRACE_COLUMN_LABELS}
                hidden={hiddenColumns}
                onChange={(next) => {
                  setHiddenColumns(next);
                  try {
                    localStorage.setItem(COLUMNS_KEY, JSON.stringify(next));
                  } catch {}
                }}
              />
              <Menu.Root>
                <Menu.Trigger
                  aria-label={`Sort: ${sort.label}`}
                  title="Sort"
                  className="flex size-7 shrink-0 items-center justify-center rounded-sm text-text-secondary outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:bg-surface-fill"
                >
                  <Icon name="sort" />
                </Menu.Trigger>
                <Menu.Portal>
                  <Menu.Positioner align="end" sideOffset={6}>
                    <Menu.Popup>
                      <Menu.Group>
                        <Menu.GroupLabel>Sort</Menu.GroupLabel>
                        <Menu.RadioGroup
                          value={sort.value}
                          onValueChange={(value: string) =>
                            setParams({ sort: value === 'newest' ? null : value, page: searchParams.get('page') })
                          }
                        >
                          {SORTS.map((item) => (
                            <Menu.RadioItem key={item.value} value={item.value}>
                              {item.label}
                            </Menu.RadioItem>
                          ))}
                        </Menu.RadioGroup>
                      </Menu.Group>
                    </Menu.Popup>
                  </Menu.Positioner>
                </Menu.Portal>
              </Menu.Root>
            </>
          }
        />
        <DataTable table={table} label="Traces" size="sm" isLoading={isLoading} />
        {data && traces.length === 0 && (
          <NoResults title="No traces found" onReset={() => setSearchParams({}, { replace: true })} />
        )}
        <Pager
          className="mt-auto"
          start={(page - 1) * PAGE_SIZE + 1}
          rowCount={traces.length}
          hasPrevious={page > 1}
          hasNext={(data?.length ?? 0) > PAGE_SIZE}
          onPrevious={() => setParams({ page: page > 2 ? String(page - 1) : null })}
          onNext={() => setParams({ page: String(page + 1) })}
        />
      </div>
    </div>
  );
}
