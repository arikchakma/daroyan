import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';
import { cva } from 'class-variance-authority';
import { Link } from 'react-router';

import { CronRunBars } from '~/components/crons/cron-run-bars';
import { CronStatusBadge } from '~/components/crons/cron-status-badge';
import { Icon } from '~/components/icons/icon';
import { EventsTile } from '~/components/overview/events-tile';
import { TelemetryTile } from '~/components/overview/telemetry-tile';
import { Tile } from '~/components/overview/tile';
import { TileRow } from '~/components/overview/tile-row';
import { TrendBars } from '~/components/overview/trend-bars';
import type { IconName } from '~/components/icons/icon-paths';
import { DetailSection } from '~/components/servers/detail-table';
import { ServerStatusBadge } from '~/components/servers/server-status-badge';
import { Button } from '~/components/ui/button';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { UptimeBars } from '~/components/uptime/uptime-bars';
import { UptimeStatusBadge } from '~/components/uptime/uptime-status-badge';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { listAlertMutesOptions, listAlertsOptions } from '~/queries/alerts';
import { listIncidentsOptions } from '~/queries/incidents';
import { overviewEventsOptions, overviewTrendsOptions } from '~/queries/overview';
import { listIssuesOptions, traceServicesOptions, traceStatusOptions } from '~/queries/traces';
import { describeFiring, getAlertMute } from '~/utils/alerts';
import { formatUntil } from '~/utils/crons';
import { DAY_MS, formatClock, formatRelative, pluralize } from '~/utils/format';
import { getStatusMeta, isClosed, isMaintenance } from '~/utils/incidents';
import {
  countCleanDays,
  describeCpu,
  getAlertLink,
  getWorstMetric,
  joinParts,
  mergeDays,
} from '~/utils/overview';
import { formatMs, formatUptime, getUptime } from '~/utils/uptime';

const CRON_BARS = 30;
const BAD_CRON_STATUSES = ['failed', 'missed', 'late'];

type OverviewDashboardProps = {
  servers: Schemas['ServerItem'][];
  checks: Schemas['UptimeCheck'][];
  crons: Schemas['Cron'][];
};

const attentionIcon = cva('shrink-0', {
  variants: {
    tone: {
      critical: 'text-tone-critical-fg',
      warning: 'text-tone-warning-fg',
    },
  },
});

type AttentionItem = {
  key: string;
  icon: IconName;
  tone: 'critical' | 'warning';
  title: string;
  detail: string;
  href: string;
  action: string;
};

export function OverviewDashboard(props: OverviewDashboardProps) {
  const { servers, checks, crons } = props;

  const isAdmin = useIsAdmin();
  const { data: firing = [] } = useQuery(listAlertsOptions('firing'));
  const { data: resolved = [] } = useQuery(listAlertsOptions('resolved'));
  const { data: mutes = [] } = useQuery(listAlertMutesOptions());
  const { data: incidents = [] } = useQuery(listIncidentsOptions());
  const { data: trends } = useQuery(overviewTrendsOptions());
  const { data: events } = useQuery(overviewEventsOptions());
  const { data: traceStatus } = useQuery(traceStatusOptions());
  const { data: services = [] } = useQuery(traceServicesOptions(DAY_MS));
  const { data: issues = [] } = useQuery(listIssuesOptions());

  const activeAlerts = firing.filter(
    (alert) => !getAlertMute(mutes, alert)
  );
  const alertedIds = new Set(activeAlerts.flatMap((alert) => [alert.server_id, alert.check_id, alert.cron_id]));
  const openIncidents = incidents.filter((incident) => !isMaintenance(incident) && !isClosed(incident.status));
  const downServers = servers.filter((server) => server.status === 'down');
  const warningServers = servers.filter((server) => server.status === 'warning');
  const activeChecks = checks.filter((check) => !check.paused);
  const badChecks = activeChecks.filter((check) => check.status === 'down' || check.status === 'degraded');
  const activeCrons = crons.filter((cron) => !cron.paused);
  const badCrons = activeCrons.filter((cron) => BAD_CRON_STATUSES.includes(cron.status));

  const attention: AttentionItem[] = [
    ...openIncidents.map((incident) => ({
      key: `incident-${incident.id}`,
      icon: 'flag' as const,
      tone: 'critical' as const,
      title: `Incident open: ${incident.title}`,
      detail: `${getStatusMeta(incident.status).label} since ${formatClock(Date.parse(incident.started_at))}.`,
      href: `/incidents/${incident.id}`,
      action: 'Open Incident',
    })),
    ...activeAlerts.map((alert) => ({
      key: `alert-${alert.id}`,
      icon: alert.severity === 'critical' ? ('circle-x' as const) : ('triangle-alert' as const),
      tone: alert.severity === 'critical' ? ('critical' as const) : ('warning' as const),
      title: `${alert.rule_name} on ${alert.target_name}`,
      detail: `${describeFiring(alert)}.`,
      ...getAlertLink(alert),
    })),
    ...downServers
      .filter((server) => !alertedIds.has(server.id))
      .map((server) => ({
        key: `server-${server.id}`,
        icon: 'circle-x' as const,
        tone: 'critical' as const,
        title: `${server.name} is down`,
        detail: server.last_seen_at ? `Last report ${formatRelative(server.last_seen_at)}.` : 'It has not reported yet.',
        href: `/servers/${server.id}`,
        action: 'Open Server',
      })),
    ...badChecks
      .filter((check) => !alertedIds.has(check.id))
      .map((check) => ({
        key: `check-${check.id}`,
        icon: 'activity' as const,
        tone: check.status === 'down' ? ('critical' as const) : ('warning' as const),
        title: check.status === 'down' ? `${check.name} is down` : `${check.name} is slow`,
        detail:
          check.status === 'down'
            ? `${check.target} is not answering.`
            : `${check.target} answers in ${formatMs(check.last_response_ms)}.`,
        href: `/uptime/${check.id}`,
        action: 'Open Check',
      })),
    ...badCrons
      .filter((cron) => !alertedIds.has(cron.id))
      .map((cron) => ({
        key: `cron-${cron.id}`,
        icon: 'clock' as const,
        tone: cron.status === 'late' ? ('warning' as const) : ('critical' as const),
        title: cron.status === 'late' ? `${cron.name} is late` : `${cron.name} ${cron.status}`,
        detail:
          cron.status === 'late' && cron.next_expected_at
            ? `It was due ${formatRelative(cron.next_expected_at)}.`
            : cron.next_expected_at && cron.next_expected_at > Date.now()
              ? `Next run ${formatUntil(cron.next_expected_at)}.`
              : 'Open it to see the last run.',
        href: `/crons/${cron.id}`,
        action: 'Open Cron',
      })),
  ];

  const lastResolved = resolved[0];
  const scope = joinParts(
    [
      servers.length > 0 ? pluralize(servers.length, 'server') : '',
      activeChecks.length > 0 ? pluralize(activeChecks.length, 'uptime check') : '',
      activeCrons.length > 0 ? pluralize(activeCrons.length, 'cron') : '',
      services.length > 0 ? pluralize(services.length, 'service') : '',
    ].filter(Boolean)
  );

  return (
    <div className="flex min-h-full flex-col bg-surface-page">
      <PageBar icon="house" title="Overview">
        <Menu.Root>
          <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
            <Icon name="plus" />
            New
            <Icon name="chevron-down" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="end">
              <Menu.Popup className="min-w-40">
                {isAdmin && (
                  <>
                    <Menu.LinkItem render={<Link to="/servers/new" />}>
                      <Icon name="server" />
                      Server
                    </Menu.LinkItem>
                    <Menu.LinkItem render={<Link to="/uptime/new" />}>
                      <Icon name="activity" />
                      Uptime Check
                    </Menu.LinkItem>
                    <Menu.LinkItem render={<Link to="/crons/new" />}>
                      <Icon name="clock" />
                      Cron
                    </Menu.LinkItem>
                  </>
                )}
                <Menu.LinkItem render={<Link to="/incidents?new=incident" />}>
                  <Icon name="flag" />
                  Incident
                </Menu.LinkItem>
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.Root>
      </PageBar>

      <div className="flex justify-center px-10 pt-3 pb-16 max-lg:px-4">
        <div className="flex w-235 max-w-full min-w-0 flex-col gap-10">
          <div className="flex flex-col gap-1">
            <h1 className="text-title-page font-semibold">
              {attention.length === 0
                ? 'All systems normal'
                : `${pluralize(attention.length, 'thing')} ${attention.length === 1 ? 'needs' : 'need'} attention`}
            </h1>
            {scope && (
              <p className="text-body text-text-secondary">
                {attention.length === 0 ? `${scope} are healthy.` : `Across ${scope}.`}
              </p>
            )}
          </div>

          <DetailSection
            title="Needs Attention"
            summary={attention.length === 0 ? 'Nothing open' : `${attention.length} open`}
          >
            <div className="overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
              {attention.length === 0 ? (
                <div className="flex gap-3 px-4 py-3">
                  <Icon name="circle-check-fill" className="mt-0.5 shrink-0 text-tone-success-fg" />
                  <div className="flex min-w-0 flex-1 flex-col gap-0.5 text-body">
                    <span className="font-medium">Nothing needs attention</span>
                    <span className="truncate text-text-secondary">
                      {lastResolved?.resolved_at
                        ? `The last problem, ${lastResolved.rule_name} on ${lastResolved.target_name}, resolved ${formatRelative(lastResolved.resolved_at)}.`
                        : 'Everything Daroyan watches is healthy.'}
                    </span>
                  </div>
                </div>
              ) : (
                attention.map((item) => (
                  <div key={item.key} className="flex items-center gap-3 px-4 py-3">
                    <Icon name={item.icon} className={attentionIcon({ tone: item.tone })} />
                    <div className="flex min-w-0 flex-1 flex-col gap-0.5 text-body">
                      <span className="truncate font-medium">{item.title}</span>
                      <span className="truncate text-text-secondary">{item.detail}</span>
                    </div>
                    <Button nativeButton={false} variant="secondary" render={<Link to={item.href} />} className="w-fit">
                      {item.action}
                    </Button>
                  </div>
                ))
              )}
            </div>
          </DetailSection>

          <div className="grid grid-cols-2 gap-4 max-lg:grid-cols-1">
            <Tile
              title="Servers"
              href="/servers"
              linkLabel={`View all ${servers.length}`}
              value={`${servers.length - downServers.length} of ${servers.length} up`}
              caption={describeCpu(trends?.cpu ?? [])}
              chart={
                <TrendBars
                  points={trends?.cpu ?? []}
                  label={(value) => `${Math.round(value)}% CPU`}
                  minScale={10}
                  max={100}
                  fillClassName="bg-chart-primary"
                  warnAt={80}
                  criticalAt={95}
                />
              }
              healthy={`All ${pluralize(servers.length, 'server')} reported in the last minute.`}
            >
              {[...downServers, ...warningServers].slice(0, 1).map((server) => (
                <TileRow
                  key={server.id}
                  more={downServers.length + warningServers.length - 1}
                  href={`/servers/${server.id}`}
                  badge={<ServerStatusBadge status={server.status} />}
                  label={server.name}
                  meta={
                    server.status === 'down'
                      ? server.last_seen_at
                        ? formatRelative(server.last_seen_at)
                        : 'never'
                      : getWorstMetric(server)
                  }
                />
              ))}
            </Tile>

            <Tile
              title="Uptime"
              href="/uptime"
              linkLabel={`View all ${checks.length}`}
              value={`${activeChecks.length - badChecks.filter((check) => check.status === 'down').length} of ${activeChecks.length} up`}
              caption={`${formatUptime(getUptime(mergeDays(activeChecks).slice(-30)))} last 30 days`}
              chart={<UptimeBars days={mergeDays(activeChecks)} className="h-7" />}
              healthy={`All checks passed for ${pluralize(countCleanDays(mergeDays(activeChecks)), 'day')}.`}
            >
              {badChecks.slice(0, 1).map((check) => (
                <TileRow
                  key={check.id}
                  more={badChecks.length - 1}
                  href={`/uptime/${check.id}`}
                  badge={<UptimeStatusBadge status={check.status} />}
                  label={check.name}
                  meta={check.status === 'down' ? check.target : formatMs(check.last_response_ms)}
                />
              ))}
            </Tile>

            <Tile
              title="Crons"
              href="/crons"
              linkLabel={`View all ${crons.length}`}
              value={`${activeCrons.length - badCrons.length} of ${activeCrons.length} on time`}
              caption={(() => {
                const next = activeCrons
                  .map((cron) => cron.next_expected_at)
                  .filter((at): at is number => typeof at === 'number' && at > Date.now())
                  .sort((a, b) => a - b)[0];
                return next ? `next run ${formatUntil(next)}` : undefined;
              })()}
              chart={
                <CronRunBars
                  runs={activeCrons
                    .flatMap((cron) => cron.runs)
                    .sort((a, b) => a.at - b.at)
                    .slice(-CRON_BARS)}
                  count={CRON_BARS}
                  className="h-7"
                />
              }
              healthy="No failed or late runs."
            >
              {badCrons.slice(0, 1).map((cron) => (
                <TileRow
                  key={cron.id}
                  more={badCrons.length - 1}
                  href={`/crons/${cron.id}`}
                  badge={<CronStatusBadge status={cron.status} />}
                  label={cron.name}
                  isMono
                  meta={
                    cron.status === 'late' && cron.next_expected_at
                      ? `due ${formatRelative(cron.next_expected_at)}`
                      : cron.last_ping_at
                        ? `at ${formatClock(cron.last_ping_at)}`
                        : ''
                  }
                />
              ))}
            </Tile>

            <TelemetryTile
              issues={issues}
              errors={trends?.errors ?? []}
              isSetUp={!!traceStatus?.last_span_at}
            />
          </div>

          <EventsTile events={events} />
        </div>
      </div>
    </div>
  );
}
