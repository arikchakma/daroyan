import { useQuery } from '@tanstack/react-query';
import type { ColumnVisibilityState } from '@tanstack/react-table';
import { useCallback, useState } from 'react';
import { useSearchParams } from 'react-router';

import { AlertBanners } from '~/components/alerts/alert-banners';
import { Icon } from '~/components/icons/icon';
import { LogSheet } from '~/components/logs/log-sheet';
import { LogVolume } from '~/components/logs/log-volume';
import { RunQueryButton } from '~/components/logs/run-query-button';
import { SaveViewDialog } from '~/components/logs/save-view-dialog';
import { SearchSyntaxPopover } from '~/components/logs/search-syntax-popover';
import { ViewMenu, VIEWS } from '~/components/logs/view-menu';
import type { LevelView } from '~/components/logs/view-menu';
import { ViewOptionsMenu } from '~/components/logs/view-options-menu';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
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
import { useStoredState } from '~/hooks/use-stored-state';
import type { LogRow } from '~/hooks/use-logs-table';
import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';
import { toast } from '~/lib/toast';
import { instanceQueries } from '~/queries/instance';
import { logQueries } from '~/queries/logs';
import { serverQueries } from '~/queries/servers';
import { cn } from '~/utils/classname';
import { getQuery, readListParam } from '~/utils/filter';
import { joinNames } from '~/utils/format';
import { getViewChips, getViewParams, isSameView } from '~/utils/log-views';
import type { SavedLogView } from '~/utils/log-views';
import type { SearchFilterField, SearchFilterValue } from '~/utils/filter';
import { formatTerm } from '~/utils/log-search';
import {
  DEFAULT_LOG_RANGE,
  getLogRange,
  getLogRanges,
  getPagingKey,
  getScanNote,
  getZoomLabel,
  OTLP_SERVER_ID,
  OTLP_SERVER_NAME,
} from '~/utils/logs';

const PAGE_SIZES = [50, 100, 200];
const EXPORT_LIMIT = 5_000;
const COLUMNS_KEY = 'daroyan.logs.columns';
const WRAP_KEY = 'daroyan.logs.wrap';
const SAVED_VIEWS_KEY = 'daroyan.logs.views';

const FILTER_KEYS = [
  { key: 'server', param: 'server_id' },
  { key: 'service', param: 'service' },
  { key: 'method', param: 'method' },
  { key: 'status', param: 'status' },
] as const;

const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'];
const STATUSES = ['2xx', '3xx', '4xx', '5xx'];

const EMPTY_FILTER: SearchFilterValue = { values: [], negate: false };

function getFilterValues(params: URLSearchParams): Record<string, SearchFilterValue> {
  return Object.fromEntries(
    FILTER_KEYS.map(({ key, param }) => [
      key,
      { values: readListParam(params.get(param)), negate: params.get(`${key}_op`) === 'not' },
    ])
  );
}

function getFilterParams(values: Record<string, SearchFilterValue>) {
  return Object.fromEntries(
    FILTER_KEYS.flatMap(({ key, param }) => {
      const value = values[key] ?? EMPTY_FILTER;
      const isSet = value.values.length > 0;

      return [
        [param, isSet ? value.values.join(',') : null],
        [`${key}_op`, isSet && value.negate ? 'not' : null],
      ];
    })
  );
}

function toOptions(values: string[]) {
  return values.map((value) => ({ value, label: value }));
}

export function meta() {
  return [{ title: 'Logs · Daroyan' }];
}

export default function LogsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const filterValues = getFilterValues(searchParams);
  const filterParams = Object.fromEntries(
    Object.entries(getFilterParams(filterValues)).filter(([, value]) => value)
  );
  const server = filterValues.server!;
  const isTelemetryView = !server.negate && server.values.includes(OTLP_SERVER_ID);
  const view = (VIEWS.find((item) => item.value === searchParams.get('level'))?.value ?? 'all') as LevelView;
  const q = searchParams.get('q') ?? '';
  const page = Math.max(Number(searchParams.get('page')) || 1, 1);
  const pageSize = PAGE_SIZES.find((size) => size === Number(searchParams.get('limit'))) ?? PAGE_SIZES[0]!;
  const { data: instance } = useQuery(instanceQueries.detail());
  const ranges = getLogRanges(instance?.archive?.retention_days);
  const widestRange = ranges[ranges.length - 1]!;
  const range = getLogRange(searchParams.get('range'));
  const zoomSince = Number(searchParams.get('from'));
  const zoomUntil = Number(searchParams.get('to'));
  const zoom = zoomSince > 0 && zoomUntil > zoomSince ? { since: zoomSince, until: zoomUntil } : undefined;

  const committedKey = JSON.stringify([q, getFilterParams(filterValues)]);
  const [search, setSearch] = useState(q);
  const [draftFilters, setDraftFilters] = useState(() => getFilterValues(searchParams));
  const [draftKey, setDraftKey] = useState(committedKey);
  if (draftKey !== committedKey) {
    setDraftKey(committedKey);
    setSearch(q);
    setDraftFilters(filterValues);
  }
  const [isLive, setIsLive] = useState(false);
  const paramsKey = searchParams.toString();
  const [timeWindow, setTimeWindow] = useState(() => ({
    key: paramsKey,
    until: Date.now(),
    nextUntil: undefined as number | undefined,
  }));
  if (timeWindow.key !== paramsKey) {
    setTimeWindow({ key: paramsKey, until: timeWindow.nextUntil ?? Date.now(), nextUntil: undefined });
  }
  const until = timeWindow.until;
  const pagingKey = getPagingKey(searchParams);
  const [cursors, setCursors] = useState(() => ({ key: pagingKey, pages: {} as Record<number, string> }));
  if (cursors.key !== pagingKey) {
    setCursors({ key: pagingKey, pages: {} });
  }
  const before = cursors.key === pagingKey ? cursors.pages[page] : undefined;
  const [openLine, setOpenLine] = useState<LogRow>();
  const [isWrapped, setIsWrapped] = useStoredState(WRAP_KEY, false);
  const [hiddenColumns, setHiddenColumns] = useStoredState<ColumnVisibilityState>(COLUMNS_KEY, {});
  const [savedViews, setSavedViews] = useStoredState<SavedLogView[]>(SAVED_VIEWS_KEY, []);
  const [isSavingView, setIsSavingView] = useState(false);
  const [removingView, setRemovingView] = useState<SavedLogView>();
  const [isExporting, setIsExporting] = useState(false);

  const commitParams = (next: URLSearchParams) => {
    if (next.toString() === paramsKey) {
      setTimeWindow((current) => ({ ...current, until: Date.now() }));
      return;
    }

    setSearchParams(next, { replace: true });
  };

  const setParams = (changes: Record<string, string | null>) => {
    const next = new URLSearchParams(searchParams);
    for (const [key, value] of Object.entries(changes)) {
      if (value) {
        next.set(key, value);
      } else {
        next.delete(key);
      }
    }
    if (!('page' in changes)) {
      next.delete('page');
    }

    commitParams(next);
  };

  const runQuery = (query: string, changes: Record<string, string | null> = {}) =>
    setParams({ ...getFilterParams(draftFilters), q: query.trim() || null, ...changes });
  const resetDraft = (params: URLSearchParams) => {
    setSearch(params.get('q') ?? '');
    setDraftFilters(getFilterValues(params));
  };

  const isFollowing = isLive && page === 1 && !zoom;
  const filters = {
    ...filterParams,
    level: view === 'all' ? undefined : view,
    q: q || undefined,
  };
  const timeRange = {
    since: zoom?.since ?? until - range.ms,
    until: zoom?.until ?? (isFollowing ? undefined : until),
  };
  const { data: servers } = useQuery(serverQueries.list());
  const { data: sources } = useQuery(logQueries.sources());
  const { data, isLoading, isPlaceholderData: isShowingOldLines } = useQuery(
    logQueries.list(
      { ...filters, ...timeRange, ...(before ? { before } : { offset: (page - 1) * pageSize }), limit: pageSize },
      isFollowing
    )
  );
  const { data: volume, isPlaceholderData: isShowingOldVolume } = useQuery(
    logQueries.volume({ ...filters, ...timeRange }, isFollowing)
  );
  const isShowingOldResults = isShowingOldLines || isShowingOldVolume;
  const isDraftChanged =
    search.trim() !== q ||
    JSON.stringify(getFilterParams(draftFilters)) !== JSON.stringify(getFilterParams(filterValues));

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
  const onZoom = useCallback(
    (since: number, zoomedUntil: number) => {
      setIsLive(false);
      setSearchParams(
        (params) => {
          params.set('from', String(since));
          params.set('to', String(zoomedUntil));
          params.delete('page');
          return params;
        },
        { replace: true }
      );
    },
    [setSearchParams]
  );
  const openIndex = openLine ? lines.findIndex((line) => line.id === openLine.id) : -1;
  const table = useLogsTable({ lines, isWrapped, columnVisibility, onOpen });

  const exportLogs = async () => {
    setIsExporting(true);
    try {
      const exportUntil = zoom?.until ?? Date.now();
      const exportSince = zoom?.since ?? exportUntil - range.ms;
      const rows = [];
      let exportBefore: string | undefined;
      while (rows.length < EXPORT_LIMIT) {
        const page = await request(
          apiClient.GET('/v1/logs', {
            params: { query: { ...filters, since: exportSince, until: exportUntil, before: exportBefore, limit: 200 } },
          })
        );
        rows.push(...page.lines);
        if (!page.has_more || !page.next) {
          break;
        }
        exportBefore = page.next;
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

  const draftServers = draftFilters.server ?? EMPTY_FILTER;
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
                !draftServers.values.length ||
                draftServers.values.includes(source.server_id) !== draftServers.negate
            )
            .map((source) => source.service)
        ),
      ]),
    },
    { key: 'method', label: 'Method', verb: 'is', multiple: true, negatable: true, options: toOptions(METHODS) },
    { key: 'status', label: 'Status', verb: 'is', multiple: true, negatable: true, options: toOptions(STATUSES) },
  ];
  const isViewChanged =
    view !== 'all' || !!q || Object.keys(filterParams).length > 0 || range.value !== DEFAULT_LOG_RANGE || !!zoom;
  const hasNoSources = sources?.length === 0;
  const viewParams = getViewParams(searchParams);
  const activeView = savedViews.find((saved) => isSameView(saved.params, viewParams));
  const canSaveView = Object.keys(viewParams).length > 0 && !activeView;
  const viewChips = getViewChips({
    levelLabel: view === 'all' ? undefined : VIEWS.find((item) => item.value === view)?.label,
    filters: fields.map((field) => ({
      label: field.label,
      negate: filterValues[field.key]!.negate,
      values: filterValues[field.key]!.values.map(
        (value) => field.options.find((option) => option.value === value)?.label ?? value
      ),
    })),
    q,
    rangeLabel: range.value === DEFAULT_LOG_RANGE ? undefined : range.label,
  });

  const applyView = (params: Record<string, string>) => {
    const next = new URLSearchParams(params);
    const limit = searchParams.get('limit');
    if (limit) {
      next.set('limit', limit);
    }

    resetDraft(next);
    commitParams(next);
  };
  const saveView = (name: string) => {
    setSavedViews([...savedViews, { id: String(Date.now()), name, params: viewParams }]);
    setIsSavingView(false);
    toast.success(`Saved ${name}`);
  };
  const removeView = (removed: SavedLogView) => {
    setSavedViews(savedViews.filter((saved) => saved.id !== removed.id));
    setRemovingView(undefined);
    toast.success(`Removed ${removed.name}`);
  };

  useCommandActions('Actions', [
    !hasNoSources && {
      label: isLive ? 'Stop Live Tail' : 'Start Live Tail',
      icon: isLive ? 'pause' : 'play',
      onSelect: () => {
        setIsLive(!isLive);
        setParams({ page: null, from: null, to: null });
      },
    },
    !hasNoSources && {
      label: isWrapped ? 'Unwrap Lines' : 'Wrap Lines',
      icon: 'align-left',
      onSelect: () => setIsWrapped(!isWrapped),
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
                {zoom ? getZoomLabel(zoom.since, zoom.until) : range.label}
                <Icon name="chevron-down" />
              </Menu.Trigger>
              <Menu.Portal>
                <Menu.Positioner align="end">
                  <Menu.Popup className="min-w-40">
                    <Menu.RadioGroup
                      value={zoom ? null : range.value}
                      onValueChange={(value: string) =>
                        setParams({ range: value === DEFAULT_LOG_RANGE ? null : value, from: null, to: null })
                      }
                    >
                      {ranges.map((item) => (
                        <Menu.RadioItem key={item.value} value={item.value}>
                          {item.label}
                        </Menu.RadioItem>
                      ))}
                    </Menu.RadioGroup>
                  </Menu.Popup>
                </Menu.Positioner>
              </Menu.Portal>
            </Menu.Root>
            <ViewOptionsMenu
              labels={COLUMN_LABELS}
              hidden={hiddenColumns}
              onHiddenChange={setHiddenColumns}
              isWrapped={isWrapped}
              onWrappedChange={setIsWrapped}
              pageSizes={PAGE_SIZES}
              pageSize={pageSize}
              onPageSizeChange={(next) => {
                setTimeWindow((current) => ({ ...current, nextUntil: current.until }));
                setParams({ limit: next === PAGE_SIZES[0] ? null : String(next) });
              }}
            />
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
                setParams({ page: null, from: null, to: null });
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
            <ViewMenu
              view={view}
              savedViews={savedViews}
              activeView={activeView}
              canSaveView={canSaveView}
              onChange={(next) => runQuery(search, { level: next === 'all' ? null : next })}
              onApplyView={(saved) => applyView(saved.params)}
              onRemoveView={setRemovingView}
              onSaveView={() => setIsSavingView(true)}
            />
            <SearchFilter
              label="Search, or -path:/v1/health"
              fields={fields}
              filters={draftFilters}
              onFilterChange={(changes) =>
                setDraftFilters((current) => ({
                  ...current,
                  ...Object.fromEntries(changes.map((change) => [change.key, change.value ?? EMPTY_FILTER])),
                }))
              }
              query={search}
              onQueryChange={setSearch}
              onSubmit={runQuery}
            />
            {canSaveView && (
              <button
                type="button"
                onClick={() => setIsSavingView(true)}
                className="flex h-7 shrink-0 items-center gap-1 rounded-sm pr-2 pl-1.5 text-body font-medium text-text-secondary outline-none hover:bg-surface-fill hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40"
              >
                <Icon name="plus" />
                Save View
              </button>
            )}
            {isViewChanged && (
              <ResetViewButton
                onReset={() => applyView({})}
              />
            )}
            <SearchSyntaxPopover />
            <RunQueryButton
              isChanged={isDraftChanged}
              isRunning={isShowingOldResults}
              onRun={() => runQuery(search)}
            />
          </div>
          <div className={cn('flex flex-col gap-2 transition-opacity', isShowingOldResults && 'opacity-60')}>
            <LogVolume volume={volume} onZoom={onZoom} />
            <DataTable table={table} label="Logs" size="sm" verticalAlign="top" isLoading={isLoading} />
          </div>
          {data && lines.length === 0 && (
            <p className="py-10 text-center text-body text-text-secondary">
              {silentServers.length > 0 ? (
                <>
                  {joinNames(silentServers)} {silentServers.length === 1 ? 'does' : 'do'} not send logs yet. Add{' '}
                  <code className="font-mono text-caption">logs: true</code> to a process in the agent config, or list a
                  file under <code className="font-mono text-caption">files</code>.
                </>
              ) : zoom ? (
                'No log lines in this range match these filters.'
              ) : range.value === widestRange.value ? (
                'No log lines match these filters.'
              ) : (
                <>
                  No log lines in the {range.label.toLowerCase()} match these filters.{' '}
                  <button
                    type="button"
                    onClick={() => setParams({ range: widestRange.value })}
                    className="text-text-link outline-none hover:underline focus-visible:underline"
                  >
                    Search the {widestRange.label.toLowerCase()}
                  </button>
                </>
              )}
            </p>
          )}
          <Pager
            className="mt-auto"
            start={(page - 1) * pageSize + 1}
            rowCount={data?.lines.length ?? 0}
            hasPrevious={page > 1}
            hasNext={!!data?.has_more}
            note={getScanNote(data?.scanned)}
            onPrevious={() => setParams({ page: page > 2 ? String(page - 1) : null })}
            onNext={() => {
              const next = data?.next;
              setIsLive(false);
              setTimeWindow((current) => ({ ...current, nextUntil: data?.until }));
              if (next) {
                setCursors((current) => ({ ...current, pages: { ...current.pages, [page + 1]: next } }));
              }
              setParams({ page: String(page + 1) });
            }}
          />
        </div>
      )}

      <SaveViewDialog open={isSavingView} onOpenChange={setIsSavingView} chips={viewChips} onSave={saveView} />
      <ConfirmDialog
        open={!!removingView}
        onOpenChange={(open) => !open && setRemovingView(undefined)}
        title={`Remove ${removingView?.name ?? 'View'}?`}
        description="Only the saved view goes. No logs are deleted."
        confirmLabel="Remove View"
        isPending={false}
        onConfirm={() => removingView && removeView(removingView)}
      />

      <LogSheet
        line={openLine}
        canGoPrevious={openIndex > 0}
        canGoNext={openIndex >= 0 && openIndex < lines.length - 1}
        onNavigate={(step) => setOpenLine(lines[openIndex + step])}
        onClose={() => setOpenLine(undefined)}
        onFilter={(term) => {
          runQuery(getQuery([search, formatTerm(term)]));
          setOpenLine(undefined);
        }}
      />
    </div>
  );
}
