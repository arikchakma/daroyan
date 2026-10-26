import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { AlertBanners } from '~/components/alerts/alert-banners';
import { DeclareIncidentSheet } from '~/components/incidents/declare-incident-sheet';
import { Icon } from '~/components/icons/icon';
import { ChartLegend, getNiceMax, MetricChart } from '~/components/servers/metric-chart';
import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { DataTable } from '~/components/ui/data-table';
import { RailRow, RailSection, Stat, StatStrip } from '~/components/ui/detail-rail';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { getCheckRules, getNewRulePath } from '~/components/uptime/check-rules';
import { UptimeBars } from '~/components/uptime/uptime-bars';
import { UptimeStatusBadge } from '~/components/uptime/uptime-status-badge';
import { useCommandActions } from '~/hooks/use-command-actions';
import { useDataTable } from '~/hooks/use-data-table';
import { useIsAdmin } from '~/hooks/use-is-admin';
import type { DataTableFeatures } from '~/lib/table';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { listAlertRulesOptions } from '~/queries/alerts';
import {
  deleteUptimeCheckOptions,
  runUptimeCheckOptions,
  toCheckInput,
  updateUptimeCheckOptions,
  uptimeCheckOptions,
  uptimeCheckStatsOptions,
} from '~/queries/uptime';
import { cn } from '~/utils/classname';
import { formatClock, formatDate, formatDuration, formatEventTime } from '~/utils/format';
import { describeExpect, describeTarget, formatInterval, formatMs, formatUptime, getUptime } from '~/utils/uptime';

import type { Route } from './+types/_app.uptime.$checkId._index';

const DAY_MS = 86_400_000;
const BUCKET_MS = 10 * 60 * 1000;

export function meta() {
  return [{ title: 'Uptime Check · Daroyan' }];
}

export default function UptimeCheckPage(props: Route.ComponentProps) {
  const { checkId } = props.params;

  const { data: check, error } = useQuery(uptimeCheckOptions(checkId));

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Check Not Found</p>
        <p className="text-body text-text-secondary">It may have been deleted.</p>
      </div>
    );
  }

  return check ? <CheckBody check={check} /> : null;
}

type CheckBodyProps = {
  check: Schemas['UptimeCheck'];
};

function CheckBody(props: CheckBodyProps) {
  const { check } = props;

  const navigate = useNavigate();
  const isAdmin = useIsAdmin();
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);
  const [isDeclareOpen, setIsDeclareOpen] = useState(false);
  const { data: details, dataUpdatedAt } = useQuery(uptimeCheckStatsOptions(check.id));
  const { data: alertRules } = useQuery(listAlertRulesOptions());

  const refresh = () => queryClient.invalidateQueries({ queryKey: ['uptime-checks'] });
  const run = useMutation({
    ...runUptimeCheckOptions(),
    meta: { errorMessage: 'The check could not run' },
    onSuccess: (result) => {
      refresh();
      if (result.status === 'down') {
        toast.error(result.error ?? 'The check failed');
      } else {
        toast.success(`${result.status === 'up' ? 'Up' : 'Degraded'} · ${formatMs(result.response_ms)}`);
      }
    },
  });
  const update = useMutation({
    ...updateUptimeCheckOptions(),
    meta: { errorMessage: 'The check could not be saved' },
    onSuccess: refresh,
  });
  const remove = useMutation({
    ...deleteUptimeCheckOptions(),
    meta: { errorMessage: 'The check could not be deleted' },
    onSuccess: () => {
      navigate('/uptime');
      queryClient.invalidateQueries({ queryKey: ['uptime-checks'], exact: true });
      toast.success(`Deleted ${check.name}`);
    },
  });

  const stats = details?.stats;
  const outages = details?.outages ?? [];
  const lastOutage = outages[0];
  const recentOutages = outages.filter((outage) => Date.parse(outage.started_at) > Date.now() - 90 * DAY_MS);
  const certDays = stats?.cert_expires_at ? Math.floor((stats.cert_expires_at - Date.now()) / DAY_MS) : undefined;
  const rules = getCheckRules(alertRules ?? [], check.id);
  const togglePause = () =>
    update.mutate({ checkId: check.id, body: { ...toCheckInput(check), paused: !check.paused } });

  useCommandActions(check.name, [
    { label: 'Run Check', icon: 'refresh', onSelect: () => run.mutate({ checkId: check.id }) },
    isAdmin && {
      label: 'Edit Check',
      icon: 'pencil',
      onSelect: () => void navigate(`/uptime/${check.id}/edit`),
    },
    isAdmin && {
      label: check.paused ? 'Resume Check' : 'Pause Check',
      icon: check.paused ? 'play' : 'pause',
      onSelect: togglePause,
    },
    isAdmin && { label: 'Delete Check', icon: 'trash', onSelect: () => setIsDeleteOpen(true) },
  ]);

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo="/uptime" icon="activity" title="Uptime">
        {isAdmin && (
          <Menu.Root>
            <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
              More Actions
              <Icon name="chevron-down" />
            </Menu.Trigger>
            <Menu.Portal>
              <Menu.Positioner align="end">
                <Menu.Popup>
                  <Menu.LinkItem render={<Link to={`/uptime/${check.id}/edit`} />}>
                    <Icon name="pencil" />
                    Edit Check
                  </Menu.LinkItem>
                  <Menu.Item onClick={togglePause}>
                    <Icon name={check.paused ? 'play' : 'pause'} />
                    {check.paused ? 'Resume Check' : 'Pause Check'}
                  </Menu.Item>
                  <Menu.Separator />
                  <Menu.Item onClick={() => setIsDeleteOpen(true)} className="text-tone-critical-fg">
                    <Icon name="trash" />
                    Delete Check
                  </Menu.Item>
                </Menu.Popup>
              </Menu.Positioner>
            </Menu.Portal>
          </Menu.Root>
        )}
        <Button className="w-fit" disabled={run.isPending} onClick={() => run.mutate({ checkId: check.id })}>
          <Icon name={run.isPending ? 'loader' : 'refresh'} className={cn(run.isPending && 'animate-spin')} />
          Run Check
        </Button>
      </PageBar>

      <div className="flex justify-center gap-10 px-10 pt-3 pb-16 max-lg:px-4">
        <div className="flex w-200 max-w-full min-w-0 flex-col gap-6">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2.5">
              <h1 className="truncate text-title-page font-semibold">{check.name}</h1>
              <UptimeStatusBadge status={check.status} />
            </div>
            <p className="truncate text-body text-text-secondary">
              {describeTarget(check)} · every {formatInterval(check.interval_secs)}
            </p>
          </div>

          <StatStrip columns={6}>
            <Stat label="Uptime" value={formatUptime(stats?.uptime)} detail="30 days" />
            <Stat label="Response" value={formatMs(stats?.average_ms)} detail="average" />
            <Stat label="p95" value={formatMs(stats?.p95_ms)} detail="30 days" />
            <Stat label="Checks" value={(stats?.checks_today ?? 0).toLocaleString('en-US')} detail="24 hours" />
            <Stat
              label="Last Outage"
              value={lastOutage ? formatDate(lastOutage.started_at) : 'None'}
              detail={lastOutage ? describeLasted(lastOutage) : undefined}
            />
            <Stat
              label="Certificate"
              value={certDays === undefined ? '—' : certDays < 0 ? 'Expired' : `${certDays} days`}
              detail={stats?.cert_expires_at ? formatDate(stats.cert_expires_at) : undefined}
            />
          </StatStrip>

          <AlertBanners checkId={check.id} />

          <DetailSection
            title="Last 90 Days"
            summary={recentOutages.length === 1 ? '1 outage' : `${recentOutages.length} outages`}
          >
            <div className="flex flex-col gap-2 rounded-2xl border border-border-ring bg-surface-default px-4 pt-4 pb-3 shadow-card">
              <UptimeBars days={check.days} className="h-8" />
              <div className="flex text-caption text-text-tertiary">
                <span className="flex-1">90 days ago</span>
                <span className="flex-1 text-center">{formatUptime(getUptime(check.days))} uptime</span>
                <span className="flex-1 text-right">Today</span>
              </div>
            </div>
          </DetailSection>

          <DetailSection title="Response Time" summary="last 24 hours">
            <ResponseCharts points={details?.points ?? []} to={dataUpdatedAt || Date.now()} />
          </DetailSection>

          <DetailSection title="Recent Checks" summary={`every ${formatInterval(check.interval_secs)}`}>
            <DetailCard>
              {details && details.recent.length > 0 ? (
                <RecentChecksTable results={details.recent} />
              ) : (
                <DetailEmpty>{check.paused ? 'This check is paused.' : 'The first check runs within a few seconds.'}</DetailEmpty>
              )}
            </DetailCard>
          </DetailSection>

          <DetailSection title="Outages" summary={outages.length > 0 ? `${recentOutages.length} in 90 days` : undefined}>
            <DetailCard>
              {outages.length > 0 ? (
                <OutagesTable outages={outages} onDeclare={() => setIsDeclareOpen(true)} />
              ) : (
                <DetailEmpty title="No outages">One starts after 2 failed checks in a row.</DetailEmpty>
              )}
            </DetailCard>
          </DetailSection>
        </div>

        <aside aria-label="Check" className="flex w-65 shrink-0 flex-col gap-4 pt-2 max-lg:hidden">
          <RailSection title="Check">
            {check.kind === 'http' && <RailRow label="Method">{check.method}</RailRow>}
            <RailRow label="Expect">{describeExpect(check)}</RailRow>
            {check.body_contains && <RailRow label="Body">contains "{check.body_contains}"</RailRow>}
            <RailRow label="Timeout">{check.timeout_secs} s</RailRow>
            <RailRow label="Interval">{formatInterval(check.interval_secs)}</RailRow>
            {isAdmin && (
              <Button nativeButton={false} variant="secondary" className="w-fit" render={<Link to={`/uptime/${check.id}/edit`} />}>
                Edit Check
              </Button>
            )}
          </RailSection>
          <RailSection title="Alerts" isLast={!isAdmin}>
            {rules.length > 0 ? (
              rules.map((rule) => (
                <Link key={rule.id} to={`/alerts/${rule.id}`} className="flex items-center gap-2 text-body hover:underline">
                  <Icon name={rule.enabled ? 'bell' : 'bell-off'} />
                  <span className="min-w-0 flex-1 truncate">{rule.name}</span>
                </Link>
              ))
            ) : (
              <p className="text-body text-text-secondary">No rule covers this check yet.</p>
            )}
            {isAdmin && (
              <Button nativeButton={false} variant="secondary" className="w-fit" render={<Link to={getNewRulePath(check.id, check.name)} />}>
                New Alert Rule
              </Button>
            )}
          </RailSection>
          {isAdmin && (
            <RailSection title="Status" isLast>
              <Button variant="secondary" className="w-fit" disabled={update.isPending} onClick={togglePause}>
                <Icon name={check.paused ? 'play' : 'pause'} />
                {check.paused ? 'Resume Check' : 'Pause Check'}
              </Button>
            </RailSection>
          )}
        </aside>
      </div>

      <DeclareIncidentSheet
        open={isDeclareOpen}
        onOpenChange={setIsDeclareOpen}
        defaults={{
          title: `${check.name} is down`,
          severity: 'major',
          components: [{ kind: 'check', id: check.id, status: 'outage' }],
          related: [{ kind: 'check', id: check.id }],
          source: `the ${check.name} outage`,
        }}
      />
      <ConfirmDialog
        open={isDeleteOpen}
        onOpenChange={setIsDeleteOpen}
        title={`Delete ${check.name}?`}
        description="Its results and outages go too."
        confirmLabel="Delete Check"
        isPending={remove.isPending}
        onConfirm={() => remove.mutate({ checkId: check.id })}
      />
    </div>
  );
}

function describeTimings(result: Schemas['UptimeResult']) {
  return [
    ['DNS', result.dns_ms],
    ['Connect', result.connect_ms],
    ['TLS', result.tls_ms],
    ['First byte', result.ttfb_ms],
  ]
    .flatMap(([label, ms]) => (ms === null || ms === undefined ? [] : [`${label} ${formatMs(ms as number)}`]))
    .join(' · ');
}

function describeLasted(outage: Schemas['UptimeOutage']) {
  if (!outage.resolved_at) {
    return 'ongoing';
  }
  return formatLasted(outage);
}

function formatLasted(outage: Schemas['UptimeOutage']) {
  const end = outage.resolved_at ? Date.parse(outage.resolved_at) : Date.now();
  const seconds = Math.round((end - Date.parse(outage.started_at)) / 1000);
  return seconds < 60 ? `${Math.max(seconds, 0)}s` : formatDuration(seconds);
}

type ResponseChartsProps = {
  points: Schemas['UptimePoint'][];
  to: number;
};

function ResponseCharts(props: ResponseChartsProps) {
  const { points, to } = props;

  const from = to - DAY_MS;
  const times = points.map((point) => point.at);
  const response = points.map((point) => point.response_ms ?? 0);
  const availability = points.map((point) => (point.total > 0 ? ((point.total - point.down) / point.total) * 100 : 100));
  const total = points.reduce((sum, point) => sum + point.total, 0);
  const down = points.reduce((sum, point) => sum + point.down, 0);
  const measured = points.flatMap((point) => (point.response_ms === null || point.response_ms === undefined ? [] : [point.response_ms]));
  const average = measured.length > 0 ? measured.reduce((sum, value) => sum + value, 0) / measured.length : null;
  const max = getNiceMax(response);
  const dns = points.map((point) => point.dns_ms ?? 0);
  const tls = points.map((point) => point.tls_ms ?? 0);
  const averageOf = (values: (number | null | undefined)[]) => {
    const present = values.filter((value): value is number => value !== null && value !== undefined);
    return present.length > 0 ? present.reduce((sum, value) => sum + value, 0) / present.length : null;
  };
  const dnsAverage = averageOf(points.map((point) => point.dns_ms));
  const tlsAverage = averageOf(points.map((point) => point.tls_ms));
  const handshakeMax = getNiceMax([...dns, ...tls]);

  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
      <MetricChart
        title="Response"
        value={formatMs(average)}
        detail="average"
        series={[{ label: 'Response', values: response }]}
        times={times}
        from={from}
        to={to}
        bucketMs={BUCKET_MS}
        max={max}
        maxLabel={formatMs(max)}
        formatValue={formatMs}
      />
      <MetricChart
        title="DNS and TLS"
        value={formatMs(dnsAverage === null && tlsAverage === null ? null : (dnsAverage ?? 0) + (tlsAverage ?? 0))}
        detail="average"
        series={[
          { label: 'DNS', values: dns },
          { label: 'TLS', values: tls, isSecondary: true },
        ]}
        times={times}
        from={from}
        to={to}
        bucketMs={BUCKET_MS}
        max={handshakeMax}
        maxLabel={formatMs(handshakeMax)}
        formatValue={formatMs}
        legend={
          <ChartLegend
            items={[
              { label: `DNS ${formatMs(dnsAverage)}` },
              { label: `TLS ${formatMs(tlsAverage)}`, isSecondary: true },
            ]}
          />
        }
      />
      <MetricChart
        title="Availability"
        value={formatUptime(total > 0 ? (total - down) / total : null)}
        detail="last 24 hours"
        isValueCritical={down > 0}
        series={[{ label: 'Availability', values: availability }]}
        times={times}
        from={from}
        to={to}
        bucketMs={BUCKET_MS}
        max={100}
        maxLabel="100%"
        formatValue={(value) => `${value.toFixed(1)}%`}
      />
    </div>
  );
}

type ResultRow = Schemas['UptimeResult'] & { id: string };

const resultHelper = createColumnHelper<DataTableFeatures, ResultRow>();

type RecentChecksTableProps = {
  results: Schemas['UptimeResult'][];
};

function RecentChecksTable(props: RecentChecksTableProps) {
  const { results } = props;

  const columns = useMemo(
    () =>
      resultHelper.columns([
        resultHelper.accessor('at', {
          header: 'Time',
          enableSorting: false,
          meta: { width: 120 },
          cell: ({ getValue }) => <span className="font-mono text-code-small">{formatEventTime(getValue())}</span>,
        }),
        resultHelper.accessor('status', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120 },
          cell: ({ getValue }) => <UptimeStatusBadge status={getValue()} />,
        }),
        resultHelper.display({
          id: 'detail',
          header: 'Detail',
          cell: ({ row }) =>
            row.original.error ? (
              <span className="block truncate text-text-secondary">{row.original.error}</span>
            ) : (
              <span className="block truncate text-text-tertiary">{describeTimings(row.original)}</span>
            ),
        }),
        resultHelper.accessor('code', {
          header: 'Code',
          enableSorting: false,
          meta: { width: 80 },
          cell: ({ getValue }) => <span className="font-mono text-code-small">{getValue() ?? '—'}</span>,
        }),
        resultHelper.accessor('response_ms', {
          header: 'Response',
          enableSorting: false,
          meta: { width: 100, align: 'right' },
          cell: ({ getValue }) => formatMs(getValue()),
        }),
      ]),
    []
  );
  const rows = useMemo(() => results.map((result, index) => ({ ...result, id: `${result.at}-${index}` })), [results]);
  const table = useDataTable({ columns, data: rows });

  return <DataTable table={table} label="Recent Checks" variant="card" size="sm" />;
}

const outageHelper = createColumnHelper<DataTableFeatures, Schemas['UptimeOutage']>();

type OutagesTableProps = {
  outages: Schemas['UptimeOutage'][];
  onDeclare: (outage: Schemas['UptimeOutage']) => void;
};

function OutagesTable(props: OutagesTableProps) {
  const { outages, onDeclare } = props;

  const columns = useMemo(
    () =>
      outageHelper.columns([
        outageHelper.accessor('started_at', {
          header: 'Started',
          enableSorting: false,
          meta: { width: 140 },
          cell: ({ getValue }) => (
            <span className="font-mono text-code-small">
              {formatDate(getValue())}, {formatClock(Date.parse(getValue()))}
            </span>
          ),
        }),
        outageHelper.accessor('cause', {
          header: 'Cause',
          enableSorting: false,
          cell: ({ getValue }) => <span className="block truncate font-medium">{getValue()}</span>,
        }),
        outageHelper.display({
          id: 'duration',
          header: 'Duration',
          meta: { width: 100, align: 'right' },
          cell: ({ row }) => formatLasted(row.original),
        }),
        outageHelper.accessor('resolved_at', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120 },
          cell: ({ getValue, row }) =>
            getValue() ? (
              <Badge tone="success">
                <Icon name="check" />
                Resolved
              </Badge>
            ) : row.original.kind === 'degraded' ? (
              <Badge tone="warning">
                <Icon name="triangle-alert" />
                Slow
              </Badge>
            ) : (
              <Badge tone="critical">
                <Icon name="circle-x" />
                Down
              </Badge>
            ),
        }),
        outageHelper.display({
          id: 'declare',
          header: '',
          meta: { width: 150, align: 'right' },
          cell: ({ row }) =>
            !row.original.resolved_at && (
              <Button variant="secondary" size="xs" className="ml-auto" onClick={() => onDeclare(row.original)}>
                <Icon name="flag" />
                Declare Incident
              </Button>
            ),
        }),
      ]),
    [onDeclare]
  );
  const table = useDataTable({ columns, data: outages });

  return <DataTable table={table} label="Outages" variant="card" size="sm" />;
}

