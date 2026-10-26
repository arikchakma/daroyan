import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { AlertBanners } from '~/components/alerts/alert-banners';
import { DeclareIncidentSheet } from '~/components/incidents/declare-incident-sheet';
import { getCronRules, getNewCronRulePath } from '~/components/crons/cron-rules';
import { CronRunBars } from '~/components/crons/cron-run-bars';
import { CronStatusBadge } from '~/components/crons/cron-status-badge';
import { RunSheet } from '~/components/crons/run-sheet';
import { Icon } from '~/components/icons/icon';
import { getNiceMax, MetricChart } from '~/components/servers/metric-chart';
import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { CodeBlock } from '~/components/ui/code-block';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { DataTable } from '~/components/ui/data-table';
import { RailRow, RailSection, Stat, StatStrip } from '~/components/ui/detail-rail';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { useCommandActions } from '~/hooks/use-command-actions';
import { useCopyToClipboard } from '~/hooks/use-copy-to-clipboard';
import { useDataTable } from '~/hooks/use-data-table';
import { useIsAdmin } from '~/hooks/use-is-admin';
import type { DataTableFeatures } from '~/lib/table';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { listAlertRulesOptions } from '~/queries/alerts';
import { cronOptions, cronStatsOptions, deleteCronOptions, toCronInput, updateCronOptions } from '~/queries/crons';
import { describeSchedule, formatRunTime, formatSecs, formatUntil, getPingUrl } from '~/utils/crons';
import { formatClock, formatDate, formatDuration, formatRelative } from '~/utils/format';
import { formatUptime } from '~/utils/uptime';

import type { Route } from './+types/_app.crons.$cronId._index';

const DAY_MS = 86_400_000;
const BUCKET_MS = 10 * 60 * 1000;
const BAR_COUNT = 90;

const RUN_SECOND_FORMAT = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', second: '2-digit' });

export function meta() {
  return [{ title: 'Cron · Daroyan' }];
}

export default function CronPage(props: Route.ComponentProps) {
  const { cronId } = props.params;

  const { data: cron, error } = useQuery(cronOptions(cronId));

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Cron Not Found</p>
        <p className="text-body text-text-secondary">It may have been deleted.</p>
      </div>
    );
  }

  return cron ? <CronBody cron={cron} /> : null;
}

type CronBodyProps = {
  cron: Schemas['Cron'];
};

function CronBody(props: CronBodyProps) {
  const { cron } = props;

  const navigate = useNavigate();
  const [copy] = useCopyToClipboard();
  const isAdmin = useIsAdmin();
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);
  const [declaring, setDeclaring] = useState<Schemas['CronFailure']>();
  const [openRun, setOpenRun] = useState<Schemas['CronRun']>();
  const { data: details, dataUpdatedAt } = useQuery(cronStatsOptions(cron.id));
  const { data: alertRules } = useQuery(listAlertRulesOptions());

  const refresh = () => queryClient.invalidateQueries({ queryKey: ['crons'] });
  const update = useMutation({
    ...updateCronOptions(),
    meta: { errorMessage: 'The cron could not be saved' },
    onSuccess: refresh,
  });
  const remove = useMutation({
    ...deleteCronOptions(),
    meta: { errorMessage: 'The cron could not be deleted' },
    onSuccess: () => {
      navigate('/crons');
      queryClient.invalidateQueries({ queryKey: ['crons'], exact: true });
      toast.success(`Deleted ${cron.name}`);
    },
  });

  const pingUrl = getPingUrl(cron.ping_key);
  const copyPingUrl = () => void copy(pingUrl).then((isCopied) => isCopied && toast.success('Copied the ping URL'));
  const togglePause = () => update.mutate({ cronId: cron.id, body: { ...toCronInput(cron), paused: !cron.paused } });

  const stats = details?.stats;
  const isWaiting = !cron.last_ping_at;
  const failures = details?.failures ?? [];
  const recentFailures = failures.filter((failure) => Date.parse(failure.started_at) > Date.now() - 90 * DAY_MS);
  const bars = details?.bars ?? [];
  const failedCount = bars.filter((bar) => bar.status === 'failed' || bar.status === 'missed').length;
  const lateCount = bars.filter((bar) => bar.status === 'late').length;
  const recent = details?.recent ?? [];
  const openIndex = openRun ? recent.indexOf(openRun) : -1;
  const rules = getCronRules(alertRules ?? [], cron.id);
  const isOverdue =
    !cron.paused && cron.status !== 'running' && !!cron.next_expected_at && cron.next_expected_at <= Date.now();
  const subtitle = [
    describeSchedule(cron),
    cron.schedule_kind === 'cron' && cron.time_zone,
    cron.server_name,
    `${formatSecs(cron.grace_secs).replace(/s$/, '')} grace`,
  ]
    .filter(Boolean)
    .join(' · ');

  useCommandActions(cron.name, [
    { label: 'Copy Ping URL', icon: 'copy', onSelect: copyPingUrl },
    isAdmin && {
      label: 'Edit Cron',
      icon: 'pencil',
      onSelect: () => void navigate(`/crons/${cron.id}/edit`),
    },
    isAdmin && {
      label: cron.paused ? 'Resume Cron' : 'Pause Cron',
      icon: cron.paused ? 'play' : 'pause',
      onSelect: togglePause,
    },
    isAdmin && { label: 'Delete Cron', icon: 'trash', onSelect: () => setIsDeleteOpen(true) },
  ]);

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo="/crons" icon="clock" title="Crons">
        {isAdmin && (
          <Menu.Root>
            <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
              More Actions
              <Icon name="chevron-down" />
            </Menu.Trigger>
            <Menu.Portal>
              <Menu.Positioner align="end">
                <Menu.Popup>
                  <Menu.LinkItem render={<Link to={`/crons/${cron.id}/edit`} />}>
                    <Icon name="pencil" />
                    Edit Cron
                  </Menu.LinkItem>
                  <Menu.Item onClick={togglePause}>
                    <Icon name={cron.paused ? 'play' : 'pause'} />
                    {cron.paused ? 'Resume Cron' : 'Pause Cron'}
                  </Menu.Item>
                  <Menu.Separator />
                  <Menu.Item onClick={() => setIsDeleteOpen(true)} className="text-tone-critical-fg">
                    <Icon name="trash" />
                    Delete Cron
                  </Menu.Item>
                </Menu.Popup>
              </Menu.Positioner>
            </Menu.Portal>
          </Menu.Root>
        )}
        <Button className="w-fit" onClick={copyPingUrl}>
          <Icon name="copy" />
          Copy Ping URL
        </Button>
      </PageBar>

      <div className="flex justify-center gap-10 px-10 pt-3 pb-16 max-lg:px-4">
        <div className="flex w-200 max-w-full min-w-0 flex-col gap-6">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2.5">
              <h1 className="truncate font-mono text-title-page font-semibold">{cron.name}</h1>
              <CronStatusBadge status={cron.status} />
            </div>
            <p className="truncate text-body text-text-secondary">{subtitle}</p>
          </div>

          <StatStrip columns={6}>
            <Stat label="Success" value={isWaiting ? '—' : formatUptime(stats?.success_rate)} detail={isWaiting ? undefined : '30 days'} />
            <Stat label="Duration" value={isWaiting ? '—' : formatRunTime(stats?.average_ms)} detail={isWaiting ? undefined : 'average'} />
            <Stat label="p95" value={isWaiting ? '—' : formatRunTime(stats?.p95_ms)} detail={isWaiting ? undefined : '30 days'} />
            <Stat
              label="Runs"
              value={isWaiting ? '—' : (stats?.runs_today ?? 0).toLocaleString('en-US')}
              detail={isWaiting ? undefined : '24 hours'}
            />
            <Stat label="Last Ping" value={cron.last_ping_at ? formatRelative(cron.last_ping_at) : '—'} />
            <Stat
              label="Next Expected"
              value={isWaiting || cron.paused || !cron.next_expected_at ? '—' : isOverdue ? 'Overdue' : formatClock(cron.next_expected_at)}
              detail={
                isOverdue && cron.next_expected_at && !isWaiting
                  ? formatRunTime(Date.now() - cron.next_expected_at)
                  : undefined
              }
            />
          </StatStrip>

          <AlertBanners cronId={cron.id} />

          {isWaiting ? (
            <FirstPing cron={cron} pingUrl={pingUrl} />
          ) : (
            <>
              <DetailSection
                title={`Last ${BAR_COUNT} Runs`}
                summary={[failedCount > 0 && `${failedCount} failed`, lateCount > 0 && `${lateCount} late`].filter(Boolean).join(' · ') || undefined}
              >
                <div className="flex flex-col gap-2 rounded-2xl border border-border-ring bg-surface-default px-4 pt-4 pb-3 shadow-card">
                  <CronRunBars runs={bars} count={BAR_COUNT} className="h-8" />
                  <div className="flex text-caption text-text-tertiary">
                    <span className="flex-1">{BAR_COUNT} runs ago</span>
                    <span className="flex-1 text-center">
                      {bars.length > 0 ? `${formatUptime(bars.filter((bar) => bar.status === 'ok').length / bars.length)} on time` : ''}
                    </span>
                    <span className="flex-1 text-right">Latest</span>
                  </div>
                </div>
              </DetailSection>

              <DetailSection title="Run Time" summary="last 24 hours">
                <RunCharts points={details?.points ?? []} to={dataUpdatedAt || Date.now()} />
              </DetailSection>

              <DetailSection title="Recent Runs" summary={describeSchedule(cron)}>
                <DetailCard>
                  {recent.length > 0 ? (
                    <RecentRunsTable runs={recent} serverName={cron.server_name} onOpen={setOpenRun} />
                  ) : (
                    <DetailEmpty title="No runs yet">Runs show up here after the job's first ping.</DetailEmpty>
                  )}
                </DetailCard>
              </DetailSection>

              <DetailSection
                title="Failures"
                summary={failures.length > 0 ? `${recentFailures.length} in 90 days` : undefined}
              >
                <DetailCard>
                  {failures.length > 0 ? (
                    <FailuresTable failures={failures} onDeclare={setDeclaring} />
                  ) : (
                    <DetailEmpty title="No failures">One is recorded when a run fails or is missed.</DetailEmpty>
                  )}
                </DetailCard>
              </DetailSection>
            </>
          )}
        </div>

        <aside aria-label="Cron" className="flex w-65 shrink-0 flex-col gap-4 pt-2 max-lg:hidden">
          <RailSection title="Schedule">
            {cron.schedule_kind === 'cron' ? (
              <>
                <RailRow label="Expression">
                  <span className="font-mono text-code">{cron.expression}</span>
                </RailRow>
                <RailRow label="Timezone">{cron.time_zone}</RailRow>
              </>
            ) : (
              <RailRow label="Every">{formatSecs(cron.interval_secs ?? 0)}</RailRow>
            )}
            <RailRow label="Grace">{formatSecs(cron.grace_secs)}</RailRow>
            <RailRow label="Max Duration">{cron.max_duration_secs ? formatSecs(cron.max_duration_secs) : 'No limit'}</RailRow>
            <RailRow label="Server">{cron.server_name ?? '—'}</RailRow>
            {cron.command && (
              <RailRow label="Command">
                <span className="font-mono text-code break-all">{cron.command}</span>
              </RailRow>
            )}
            {isAdmin && (
              <Button nativeButton={false} variant="secondary" className="w-fit" render={<Link to={`/crons/${cron.id}/edit`} />}>
                Edit Cron
              </Button>
            )}
          </RailSection>
          <RailSection title="Ping URL">
            <span className="truncate text-body">{pingUrl.replace(/^https?:\/\//, '')}</span>
            <span className="text-body text-text-secondary">Add /start or /fail</span>
            <Button variant="secondary" className="w-fit" onClick={copyPingUrl}>
              Copy Ping URL
            </Button>
          </RailSection>
          <RailSection title="Alerts">
            {rules.length > 0 ? (
              rules.map((rule) => (
                <Link key={rule.id} to={`/alerts/${rule.id}`} className="flex items-center gap-2 text-body hover:underline">
                  <Icon name={rule.enabled ? 'bell' : 'bell-off'} />
                  <span className="min-w-0 flex-1 truncate">{rule.name}</span>
                </Link>
              ))
            ) : (
              <p className="text-body text-text-secondary">No rule covers this cron yet.</p>
            )}
            {(isAdmin || rules.length > 0) && (
              <Button
                nativeButton={false}
                variant="secondary"
                className="w-fit"
                render={<Link to={rules.length > 0 ? '/alerts' : getNewCronRulePath(cron.id, cron.name)} />}
              >
                {rules.length > 0 ? 'View Rules' : 'New Alert Rule'}
              </Button>
            )}
          </RailSection>
          <RailSection title="Status" isLast>
            <p className="text-body text-text-secondary">
              {cron.paused
                ? 'Paused. Pings are still recorded, but nothing alerts.'
                : `${cron.next_expected_at && !isOverdue ? `Next run at ${formatClock(cron.next_expected_at)}. ` : ''}Pausing stops the late and failed alerts.`}
            </p>
            {isAdmin && (
              <Button variant="secondary" className="w-fit" disabled={update.isPending} onClick={togglePause}>
                <Icon name={cron.paused ? 'play' : 'pause'} />
                {cron.paused ? 'Resume Cron' : 'Pause Cron'}
              </Button>
            )}
          </RailSection>
        </aside>
      </div>

      <RunSheet
        cron={cron}
        run={openRun}
        canGoPrevious={openIndex > 0}
        canGoNext={openIndex >= 0 && openIndex < recent.length - 1}
        onNavigate={(step) => setOpenRun(recent[openIndex + step])}
        onClose={() => setOpenRun(undefined)}
      />

      <DeclareIncidentSheet
        open={!!declaring}
        onOpenChange={(open) => !open && setDeclaring(undefined)}
        defaults={{
          title: `${cron.name} ${declaring?.kind === 'missed' ? 'did not run' : 'failed'}`,
          severity: 'major',
          components: [{ kind: 'cron', id: cron.id, status: declaring?.kind === 'missed' ? 'degraded' : 'outage' }],
          related: [{ kind: 'cron', id: cron.id }],
          source: `the ${cron.name} ${declaring?.kind === 'missed' ? 'missed run' : 'failure'}`,
        }}
      />
      <ConfirmDialog
        open={isDeleteOpen}
        onOpenChange={setIsDeleteOpen}
        title={`Delete ${cron.name}?`}
        description="Its runs and failures go too, and its ping URL stops working."
        confirmLabel="Delete Cron"
        isPending={remove.isPending}
        onConfirm={() => remove.mutate({ cronId: cron.id })}
      />
    </div>
  );
}

type FirstPingProps = {
  cron: Schemas['Cron'];
  pingUrl: string;
};

function FirstPing(props: FirstPingProps) {
  const { cron, pingUrl } = props;

  const schedule = cron.schedule_kind === 'cron' ? cron.expression : '0 * * * *';
  const command = cron.command ?? 'cmd';

  return (
    <DetailSection title="Send the First Ping">
      <p className="-mt-2 text-body text-text-secondary">Add one of these to the job. Nothing shows here until it pings.</p>
      <div className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
        <SetupBlock
          title="Ping URL"
          description="Call it when the job finishes. Add /start when it begins, and /fail or the exit code when it ends."
          code={pingUrl}
        />
        <SetupBlock
          title="Crontab"
          description="Put curl after your command, so the ping only goes out when the job succeeds."
          code={`${schedule} ${command} && curl -fsS ${pingUrl}`}
        />
        <SetupBlock
          title="With the Exit Code and Output"
          description="Sends /start, then the exit code with up to 10 kB of output, so failures show what went wrong."
          code={`curl -fsS ${pingUrl}/start; out=$(${command} 2>&1); curl -fsS --data-raw "$out" ${pingUrl}/$?`}
        />
        <div className="flex items-center gap-2 bg-surface-subtle px-4 py-3 text-body text-text-secondary">
          <Icon name="clock" />
          Daroyan starts watching after the first ping.
        </div>
      </div>
    </DetailSection>
  );
}

type SetupBlockProps = {
  title: string;
  description: string;
  code: string;
};

function SetupBlock(props: SetupBlockProps) {
  const { title, description, code } = props;

  return (
    <div className="flex flex-col gap-3 p-4">
      <div className="flex flex-col gap-0.5">
        <span className="text-body font-medium">{title}</span>
        <span className="text-body text-text-secondary">{description}</span>
      </div>
      <CodeBlock label={title} code={code} language="shellscript" />
    </div>
  );
}

type RunChartsProps = {
  points: Schemas['CronPoint'][];
  to: number;
};

function RunCharts(props: RunChartsProps) {
  const { points, to } = props;

  const from = to - DAY_MS;
  const times = points.map((point) => point.at);
  const durations = points.map((point) => point.duration_ms ?? 0);
  const delays = points.map((point) => Math.max(point.delay_ms ?? 0, 0));
  const success = points.map((point) => (point.total > 0 ? (point.ok / point.total) * 100 : 100));
  const averageOf = (values: (number | null | undefined)[]) => {
    const present = values.filter((value): value is number => value !== null && value !== undefined);
    return present.length > 0 ? present.reduce((sum, value) => sum + value, 0) / present.length : null;
  };
  const total = points.reduce((sum, point) => sum + point.total, 0);
  const ok = points.reduce((sum, point) => sum + point.ok, 0);
  const durationMax = getNiceMax(durations);
  const delayMax = getNiceMax(delays);
  const delayAverage = averageOf(points.map((point) => point.delay_ms));

  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
      <MetricChart
        title="Duration"
        value={formatRunTime(averageOf(points.map((point) => point.duration_ms)))}
        detail="average"
        series={[{ label: 'Duration', values: durations }]}
        times={times}
        from={from}
        to={to}
        bucketMs={BUCKET_MS}
        max={durationMax}
        maxLabel={formatRunTime(durationMax)}
        formatValue={formatRunTime}
      />
      <MetricChart
        title="Start Delay"
        value={formatRunTime(delayAverage === null ? null : Math.max(delayAverage, 0))}
        detail="after the schedule"
        series={[{ label: 'Delay', values: delays }]}
        times={times}
        from={from}
        to={to}
        bucketMs={BUCKET_MS}
        max={delayMax}
        maxLabel={formatRunTime(delayMax)}
        formatValue={formatRunTime}
      />
      <MetricChart
        title="Success Rate"
        value={formatUptime(total > 0 ? ok / total : null)}
        detail="last 24 hours"
        isValueCritical={ok < total}
        series={[{ label: 'Success', values: success }]}
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

type RunRow = Schemas['CronRun'] & { id: string };

const runHelper = createColumnHelper<DataTableFeatures, RunRow>();

type RecentRunsTableProps = {
  runs: Schemas['CronRun'][];
  serverName?: string | null;
  onOpen: (run: Schemas['CronRun']) => void;
};

function RecentRunsTable(props: RecentRunsTableProps) {
  const { runs, serverName, onOpen } = props;

  const rows = useMemo(() => runs.map((run, index) => ({ ...run, id: `${run.finished_at}-${index}` })), [runs]);
  const columns = useMemo(
    () =>
      runHelper.columns([
        runHelper.display({
          id: 'started',
          header: 'Started',
          meta: { width: 120 },
          cell: ({ row }) => (
            <button
              type="button"
              onClick={() => onOpen(runs[row.index] ?? row.original)}
              className="block w-full text-left font-mono text-code-small outline-none after:absolute after:inset-0 focus-visible:underline"
            >
              {RUN_SECOND_FORMAT.format(new Date(row.original.started_at ?? row.original.scheduled_at ?? row.original.finished_at))}
            </button>
          ),
        }),
        runHelper.display({
          id: 'server',
          header: 'Server',
          cell: () => <span className="block truncate">{serverName ?? '—'}</span>,
        }),
        runHelper.accessor('status', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120 },
          cell: ({ getValue }) => <CronStatusBadge status={getValue()} />,
        }),
        runHelper.accessor('exit_code', {
          header: 'Exit',
          enableSorting: false,
          meta: { width: 80 },
          cell: ({ getValue }) => <span className="font-mono text-code-small">{getValue() ?? '—'}</span>,
        }),
        runHelper.accessor('duration_ms', {
          header: 'Duration',
          enableSorting: false,
          meta: { width: 100, align: 'right' },
          cell: ({ getValue }) => formatRunTime(getValue()),
        }),
      ]),
    [runs, serverName, onOpen]
  );
  const table = useDataTable({ columns, data: rows });

  return <DataTable table={table} label="Recent Runs" variant="card" size="sm" />;
}

const failureHelper = createColumnHelper<DataTableFeatures, Schemas['CronFailure']>();

function formatLasted(failure: Schemas['CronFailure']) {
  const end = failure.resolved_at ? Date.parse(failure.resolved_at) : Date.now();
  const seconds = Math.round((end - Date.parse(failure.started_at)) / 1000);
  return seconds < 60 ? `${Math.max(seconds, 0)}s` : formatDuration(seconds);
}

type FailuresTableProps = {
  failures: Schemas['CronFailure'][];
  onDeclare: (failure: Schemas['CronFailure']) => void;
};

function FailuresTable(props: FailuresTableProps) {
  const { failures, onDeclare } = props;

  const columns = useMemo(
    () =>
      failureHelper.columns([
        failureHelper.accessor('started_at', {
          header: 'Started',
          enableSorting: false,
          meta: { width: 140 },
          cell: ({ getValue }) => (
            <span className="font-mono text-code-small">
              {formatDate(getValue())}, {formatClock(Date.parse(getValue()))}
            </span>
          ),
        }),
        failureHelper.display({
          id: 'cause',
          header: 'Cause',
          cell: ({ row }) => (
            <span className="flex min-w-0 flex-col gap-0.5">
              <span className="truncate font-medium">{row.original.kind === 'missed' ? 'Missed run' : 'Failed run'}</span>
              <span className="truncate font-mono text-code-small text-text-tertiary">{row.original.cause}</span>
            </span>
          ),
        }),
        failureHelper.display({
          id: 'duration',
          header: 'Duration',
          meta: { width: 100, align: 'right' },
          cell: ({ row }) => formatLasted(row.original),
        }),
        failureHelper.accessor('resolved_at', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 120 },
          cell: ({ getValue }) =>
            getValue() ? (
              <Badge tone="success">
                <Icon name="check" />
                Resolved
              </Badge>
            ) : (
              <Badge tone="critical">
                <Icon name="circle-x" />
                Ongoing
              </Badge>
            ),
        }),
        failureHelper.display({
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
  const table = useDataTable({ columns, data: failures });

  return <DataTable table={table} label="Failures" variant="card" size="lg" />;
}
