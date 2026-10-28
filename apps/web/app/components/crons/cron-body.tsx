import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { AlertBanners } from '~/components/alerts/alert-banners';
import { DeclareIncidentSheet } from '~/components/incidents/declare-incident-sheet';
import { getCronRules, getNewCronRulePath } from '~/components/crons/cron-rules';
import { CronRunBars } from '~/components/crons/cron-run-bars';
import { CronStatusBadge } from '~/components/crons/cron-status-badge';
import { FailuresTable } from '~/components/crons/failures-table';
import { FirstPing } from '~/components/crons/first-ping';
import { RecentRunsTable } from '~/components/crons/recent-runs-table';
import { RunCharts } from '~/components/crons/run-charts';
import { RunSheet } from '~/components/crons/run-sheet';
import { Icon } from '~/components/icons/icon';
import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { Button } from '~/components/ui/button';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { RailRow, RailSection, Stat, StatStrip } from '~/components/ui/detail-rail';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { useCommandActions } from '~/hooks/use-command-actions';
import { useCopyToClipboard } from '~/hooks/use-copy-to-clipboard';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { listAlertRulesOptions } from '~/queries/alerts';
import { cronStatsOptions, deleteCronOptions, toCronInput, updateCronOptions } from '~/queries/crons';
import { describeSchedule, formatRunTime, formatSecs, getPingUrl } from '~/utils/crons';
import { DAY_MS, formatClock, formatRelative } from '~/utils/format';
import { formatUptime } from '~/utils/uptime';

const BAR_COUNT = 90;

type CronBodyProps = {
  cron: Schemas['Cron'];
};

export function CronBody(props: CronBodyProps) {
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
