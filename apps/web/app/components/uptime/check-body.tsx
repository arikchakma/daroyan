import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { AlertBanners } from '~/components/alerts/alert-banners';
import { DeclareIncidentSheet } from '~/components/incidents/declare-incident-sheet';
import { Icon } from '~/components/icons/icon';
import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { Button } from '~/components/ui/button';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { RailRow, RailSection, Stat, StatStrip } from '~/components/ui/detail-rail';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { getCheckRules, getNewRulePath } from '~/components/uptime/check-rules';
import { OutagesTable } from '~/components/uptime/outages-table';
import { RecentChecksTable } from '~/components/uptime/recent-checks-table';
import { ResponseCharts } from '~/components/uptime/response-charts';
import { UptimeBars } from '~/components/uptime/uptime-bars';
import { UptimeStatusBadge } from '~/components/uptime/uptime-status-badge';
import { useCommandActions } from '~/hooks/use-command-actions';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { listAlertRulesOptions } from '~/queries/alerts';
import { deleteUptimeCheckOptions, runUptimeCheckOptions, toCheckInput, updateUptimeCheckOptions, uptimeCheckStatsOptions } from '~/queries/uptime';
import { cn } from '~/utils/classname';
import { DAY_MS, formatDate } from '~/utils/format';
import { describeExpect, describeTarget, formatInterval, formatLasted, formatMs, formatStreak, formatUptime, getUptime } from '~/utils/uptime';

type CheckBodyProps = {
  check: Schemas['UptimeCheck'];
};

export function CheckBody(props: CheckBodyProps) {
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
      if (result.status === 'down' || result.status === 'confirming') {
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
                <DetailEmpty title="No outages">
                  One starts after {formatStreak(check.failure_threshold, 'failed check', 'failed checks')}.
                </DetailEmpty>
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
            <RailRow label="Down After">{formatStreak(check.failure_threshold, 'failure', 'failures')}</RailRow>
            <RailRow label="Up After">{formatStreak(check.success_threshold, 'success', 'successes')}</RailRow>
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

function describeLasted(outage: Schemas['UptimeOutage']) {
  if (!outage.resolved_at) {
    return 'ongoing';
  }
  return formatLasted(outage);
}
