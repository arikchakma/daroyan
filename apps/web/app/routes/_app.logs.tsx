import { useQuery } from '@tanstack/react-query';
import type { ColumnVisibilityState } from '@tanstack/react-table';
import { useCallback, useEffect, useState } from 'react';
import { useSearchParams } from 'react-router';

import { AlertBanners } from '~/components/alerts/alert-banners';
import { Icon } from '~/components/icons/icon';
import { LogSheet } from '~/components/logs/log-sheet';
import { ViewMenu, VIEWS } from '~/components/logs/view-menu';
import type { LevelView } from '~/components/logs/view-menu';
import { ColumnMenu } from '~/components/ui/column-menu';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState } from '~/components/ui/empty-state';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { Pager } from '~/components/ui/pager';
import { ResetViewButton } from '~/components/ui/reset-view-button';
import { SearchFilter } from '~/components/search/search-filter';
import { Button } from '~/components/ui/button';
import { useCommandActions } from '~/hooks/use-command-actions';
import { collapseRepeats, COLUMN_LABELS, hasHttp, useLogsTable } from '~/hooks/use-logs-table';
import type { LogRow } from '~/hooks/use-logs-table';
import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';
import { toast } from '~/lib/toast';
import { logsOptions, logSourcesOptions } from '~/queries/logs';
import { listServersOptions } from '~/queries/servers';
import { cn } from '~/utils/classname';
import { joinNames } from '~/utils/alerts';
import { readListParam } from '~/utils/filter';
import type { SearchFilterField } from '~/utils/filter';
import { OTLP_SERVER_ID, OTLP_SERVER_NAME } from '~/utils/logs';

const PAGE_SIZE = 50;
const EXPORT_LIMIT = 5_000;
const COLUMNS_KEY = 'daroyan.logs.columns';
const WRAP_KEY = 'daroyan.logs.wrap';

function readStored<T>(key: string, fallback: T): T {
  try {
    const value = localStorage.getItem(key);
    return value ? (JSON.parse(value) as T) : fallback;
  } catch {
    return fallback;
  }
}

function store(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {}
}

const FILTER_KEYS = [
  { key: 'server', param: 'server_id' },
  { key: 'service', param: 'service' },
  { key: 'method', param: 'method' },
  { key: 'status', param: 'status' },
] as const;

const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'];
const STATUSES = ['2xx', '3xx', '4xx', '5xx'];

type LogRange = '15m' | '1h' | '24h' | '7d';

const DEFAULT_RANGE: LogRange = '24h';

const RANGES: { value: LogRange; label: string; ms: number }[] = [
  { value: '15m', label: 'Last 15 Minutes', ms: 15 * 60_000 },
  { value: '1h', label: 'Last Hour', ms: 3_600_000 },
  { value: '24h', label: 'Last 24 Hours', ms: 24 * 3_600_000 },
  { value: '7d', label: 'Last 7 Days', ms: 7 * 24 * 3_600_000 },
];

function toOptions(values: string[]) {
  return values.map((value) => ({ value, label: value }));
}

export function meta() {
  return [{ title: 'Logs · Daroyan' }];
}

export default function LogsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const filterValues = Object.fromEntries(
    FILTER_KEYS.map(({ key, param }) => [
      key,
      { values: readListParam(searchParams.get(param)), negate: searchParams.get(`${key}_op`) === 'not' },
    ])
  );
  const filterParams = Object.fromEntries(
    FILTER_KEYS.flatMap(({ key, param }) =>
      filterValues[key]!.values.length
        ? [
            [param, filterValues[key]!.values.join(',')],
            ...(filterValues[key]!.negate ? [[`${key}_op`, 'not']] : []),
          ]
        : []
    )
  );
  const server = filterValues.server!;
  const isTelemetryView = !server.negate && server.values.includes(OTLP_SERVER_ID);
  const view = (VIEWS.find((item) => item.value === searchParams.get('level'))?.value ?? 'all') as LevelView;
  const q = searchParams.get('q') ?? '';
  const page = Math.max(Number(searchParams.get('page')) || 1, 1);
  const range = RANGES.find((item) => item.value === searchParams.get('range')) ?? RANGES.find((item) => item.value === DEFAULT_RANGE)!;

  const [search, setSearch] = useState(q);
  const [isLive, setIsLive] = useState(false);
  const [until, setUntil] = useState(() => Date.now());
  const [openLine, setOpenLine] = useState<LogRow>();
  const [isWrapped, setIsWrapped] = useState(() => readStored(WRAP_KEY, false));
  const [hiddenColumns, setHiddenColumns] = useState<ColumnVisibilityState>(() => readStored(COLUMNS_KEY, {}));
  const [isExporting, setIsExporting] = useState(false);

  const setParams = (changes: Record<string, string | null>) => {
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
    setUntil(Date.now());
  };

  useEffect(() => {
    if (search === q) {
      return;
    }
    const timer = setTimeout(() => setParams({ q: search.trim() || null }), 300);
    return () => clearTimeout(timer);
  }, [search]);

  const isFollowing = isLive && page === 1;
  const { data: servers } = useQuery(listServersOptions());
  const { data: sources } = useQuery(logSourcesOptions());
  const { data, isLoading } = useQuery(
    logsOptions(
      {
        ...filterParams,
        level: view === 'all' ? undefined : view,
        q: q || undefined,
        since: until - range.ms,
        until: isFollowing ? undefined : until,
        offset: (page - 1) * PAGE_SIZE,
        limit: PAGE_SIZE,
      },
      isFollowing
    )
  );

  const serverNames = new Map([
    [OTLP_SERVER_ID, OTLP_SERVER_NAME],
    ...(servers ?? []).map((server) => [server.id, server.name] as const),
  ]);
  const hasOtlpLines = (sources ?? []).some((source) => source.server_id === OTLP_SERVER_ID);
  const lines = collapseRepeats(data?.lines ?? [], serverNames);
  const hasRequests = (data?.lines ?? []).some(hasHttp);
  const columnVisibility: ColumnVisibilityState = {
    ...hiddenColumns,
    ...(!hasRequests && { status: false, method: false, path: false, duration: false }),
  };
  const onOpen = useCallback((line: LogRow) => setOpenLine(line), []);
  const openIndex = openLine ? lines.findIndex((line) => line.id === openLine.id) : -1;
  const table = useLogsTable({ lines, isWrapped, columnVisibility, onOpen });

  const filters = {
    ...filterParams,
    level: view === 'all' ? undefined : view,
    q: q || undefined,
  };
  const exportLogs = async () => {
    setIsExporting(true);
    try {
      const exportUntil = Date.now();
      const rows = [];
      for (let offset = 0; offset < EXPORT_LIMIT; offset += 200) {
        const page = await request(
          apiClient.GET('/v1/logs', {
            params: { query: { ...filters, since: exportUntil - range.ms, until: exportUntil, offset, limit: 200 } },
          })
        );
        rows.push(...page.lines);
        if (!page.has_more) {
          break;
        }
      }
      const blob = new Blob([rows.map((row) => JSON.stringify(row)).join('\n')], { type: 'application/x-ndjson' });
      const link = document.createElement('a');
      link.href = URL.createObjectURL(blob);
      link.download = `daroyan-logs-${new Date(exportUntil).toISOString().slice(0, 19)}.ndjson`;
      link.click();
      URL.revokeObjectURL(link.href);
      toast.success(`Exported ${rows.length} lines`);
    } catch {
      toast.error('The logs could not be exported');
    } finally {
      setIsExporting(false);
    }
  };

  const selectedServers = filterValues.server!;
  const pickedServers = filterValues.server!;
  const silentServers = pickedServers.negate
    ? []
    : pickedServers.values
        .filter((id) => sources && !sources.some((source) => source.server_id === id))
        .map((id) => serverNames.get(id) ?? id);
  const fields: SearchFilterField[] = [
    {
      key: 'server',
      label: 'Server',
      verb: 'is',
      multiple: true,
      negatable: true,
      plural: 'servers',
      isLoading: !servers,
      options: [
        ...(servers ?? []).map((server) => ({ value: server.id, label: server.name })),
        ...(hasOtlpLines ? [{ value: OTLP_SERVER_ID, label: OTLP_SERVER_NAME }] : []),
      ],
    },
    {
      key: 'service',
      label: 'Source',
      verb: 'is',
      multiple: true,
      negatable: true,
      plural: 'sources',
      isLoading: !sources,
      options: toOptions([
        ...new Set(
          (sources ?? [])
            .filter(
              (source) =>
                !selectedServers.values.length ||
                selectedServers.values.includes(source.server_id) !== selectedServers.negate
            )
            .map((source) => source.service)
        ),
      ]),
    },
    { key: 'method', label: 'Method', verb: 'is', multiple: true, negatable: true, options: toOptions(METHODS) },
    { key: 'status', label: 'Status', verb: 'is', multiple: true, negatable: true, options: toOptions(STATUSES) },
  ];
  const isViewChanged =
    view !== 'all' || !!q || Object.keys(filterParams).length > 0 || range.value !== DEFAULT_RANGE;
  const hasNoSources = sources?.length === 0;

  useCommandActions('Actions', [
    !hasNoSources && {
      label: isLive ? 'Stop Live Tail' : 'Start Live Tail',
      icon: isLive ? 'pause' : 'play',
      onSelect: () => {
        setIsLive(!isLive);
        setParams({ page: null });
      },
    },
    !hasNoSources && {
      label: isWrapped ? 'Unwrap Lines' : 'Wrap Lines',
      icon: 'align-left',
      onSelect: () => {
        setIsWrapped(!isWrapped);
        store(WRAP_KEY, !isWrapped);
      },
    },
    !hasNoSources && !isExporting && { label: 'Export Logs', icon: 'download', onSelect: () => void exportLogs() },
  ]);

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="terminal" title="Logs">
        {!hasNoSources && (
          <>
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
                      onValueChange={(value: LogRange) => setParams({ range: value === DEFAULT_RANGE ? null : value })}
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
            <Button type="button" variant="secondary" className="w-fit" disabled={isExporting} onClick={() => void exportLogs()}>
              <Icon name={isExporting ? 'loader' : 'download'} className={cn(isExporting && 'animate-spin')} />
              Export
            </Button>
            <Button
              type="button"
              variant="secondary"
              className="w-fit"
              aria-pressed={isLive}
              onClick={() => {
                setIsLive(!isLive);
                setParams({ page: null });
              }}
            >
              <span className={cn('size-1.5 rounded-full', isLive ? 'bg-tone-success-fg' : 'bg-text-tertiary')} />
              Live
            </Button>
          </>
        )}
      </PageBar>

      {hasNoSources ? (
        <EmptyState
          icon="terminal"
          marks={['check', 'check', 'check']}
          title="No logs yet"
          description="Turn on logs for a docker or systemd process in the agent config, list a log file to follow, or send logs with an OpenTelemetry SDK."
        />
      ) : (
        <div className="flex flex-1 flex-col gap-2 px-3 pt-1 pb-3">
          {isTelemetryView && <AlertBanners serverId={OTLP_SERVER_ID} />}
          <div className="flex h-9 w-full items-center gap-0.5 rounded-lg bg-surface-default p-1 shadow-filter-bar ring-1 ring-border-elevated ring-inset">
            <ViewMenu view={view} onChange={(next) => setParams({ level: next === 'all' ? null : next })} />
            <SearchFilter
              label="Search, or -path:/v1/health"
              fields={fields}
              filters={filterValues}
              onFilterChange={(change) => {
                const { key, param } = FILTER_KEYS.find((item) => item.key === change.key)!;
                setParams({
                  [param]: change.value?.values.join(',') || null,
                  [`${key}_op`]: change.value?.negate ? 'not' : null,
                });
              }}
              query={search}
              onQueryChange={setSearch}
            />
            {isViewChanged && (
              <ResetViewButton
                onReset={() => {
                  setSearch('');
                  setSearchParams({}, { replace: true });
                  setUntil(Date.now());
                }}
              />
            )}
            <ColumnMenu
              labels={COLUMN_LABELS}
              hidden={hiddenColumns}
              onChange={(next) => {
                setHiddenColumns(next);
                store(COLUMNS_KEY, next);
              }}
            />
            <button
              type="button"
              aria-label="Wrap Lines"
              aria-pressed={isWrapped}
              title="Wrap lines"
              onClick={() => {
                setIsWrapped(!isWrapped);
                store(WRAP_KEY, !isWrapped);
              }}
              className="flex size-7 shrink-0 items-center justify-center rounded-sm text-text-secondary outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 aria-pressed:bg-surface-fill aria-pressed:text-text-primary"
            >
              <Icon name="align-left" />
            </button>
          </div>
          <DataTable table={table} label="Logs" size="sm" isLoading={isLoading} />
          {data && lines.length === 0 && (
            <p className="py-10 text-center text-body text-text-secondary">
              {silentServers.length > 0 ? (
                <>
                  {joinNames(silentServers)} {silentServers.length === 1 ? 'does' : 'do'} not send logs yet. Add{' '}
                  <code className="font-mono text-caption">logs: true</code> to a process in the agent config, or list a
                  file under <code className="font-mono text-caption">files</code>.
                </>
              ) : range.value === '7d' ? (
                'No log lines match these filters.'
              ) : (
                <>
                  No log lines in the {range.label.toLowerCase()} match these filters.{' '}
                  <button
                    type="button"
                    onClick={() => setParams({ range: '7d' })}
                    className="text-text-link outline-none hover:underline focus-visible:underline"
                  >
                    Search the last 7 days
                  </button>
                </>
              )}
            </p>
          )}
          <Pager
            className="mt-auto"
            start={(page - 1) * PAGE_SIZE + 1}
            rowCount={data?.lines.length ?? 0}
            hasPrevious={page > 1}
            hasNext={!!data?.has_more}
            onPrevious={() => setParams({ page: page > 2 ? String(page - 1) : null })}
            onNext={() => {
              setIsLive(false);
              setSearchParams(
                (params) => {
                  params.set('page', String(page + 1));
                  return params;
                },
                { replace: true }
              );
              if (data) {
                setUntil(data.until);
              }
            }}
          />
        </div>
      )}

      <LogSheet
        line={openLine}
        canGoPrevious={openIndex > 0}
        canGoNext={openIndex >= 0 && openIndex < lines.length - 1}
        onNavigate={(step) => setOpenLine(lines[openIndex + step])}
        onClose={() => setOpenLine(undefined)}
      />
    </div>
  );
}
