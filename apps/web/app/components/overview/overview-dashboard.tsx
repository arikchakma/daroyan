import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { Link } from 'react-router';

import { CronRunBars } from '~/components/crons/cron-run-bars';
import { CronStatusBadge } from '~/components/crons/cron-status-badge';
import { Icon } from '~/components/icons/icon';
import { EventsTile } from '~/components/overview/events-tile';
import type { IconName } from '~/components/icons/icon-paths';
import { DetailSection } from '~/components/servers/detail-table';
import { ServerStatusBadge } from '~/components/servers/server-status-badge';
import { Badge } from '~/components/ui/badge';
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
import { describeFiring, findMute } from '~/utils/alerts';
import { cn } from '~/utils/classname';
import { formatUntil } from '~/utils/crons';
import { DAY_MS, formatClock, formatCount, formatRelative } from '~/utils/format';
import { getStatusMeta, isClosed, isMaintenance } from '~/utils/incidents';
import { formatMs, formatUptime, getUptime } from '~/utils/uptime';

const CRON_BARS = 30;
const BAD_CRON_STATUSES = ['failed', 'missed', 'late'];

type OverviewDashboardProps = {
  servers: Schemas['ServerItem'][];
  checks: Schemas['UptimeCheck'][];
  crons: Schemas['Cron'][];
};

type AttentionItem = {
  key: string;
  icon: IconName;
  iconClassName: string;
  title: string;
  detail: string;
  href: string;
  action: string;
};

const CRITICAL_ICON = 'text-tone-critical-fg';
const WARNING_ICON = 'text-tone-warning-fg';

function getAlertLink(alert: Schemas['Alert']) {
  if (alert.server_id) {
    return { href: `/servers/${alert.server_id}`, action: 'Open Server' };
  }
  if (alert.check_id) {
    return { href: `/uptime/${alert.check_id}`, action: 'Open Check' };
  }
  if (alert.cron_id) {
    return { href: `/crons/${alert.cron_id}`, action: 'Open Cron' };
  }
  return { href: `/alerts/${alert.rule_id}`, action: 'Open Alert' };
}

function pluralize(count: number, word: string) {
  return `${formatCount(count)} ${count === 1 ? word : `${word}s`}`;
}

function joinParts(parts: string[]) {
  return parts.length <= 1 ? (parts[0] ?? '') : `${parts.slice(0, -1).join(', ')} and ${parts.at(-1)}`;
}

function getWorstMetric(server: Schemas['ServerItem']) {
  const metrics = [
    { label: 'CPU', value: server.cpu },
    { label: 'Memory', value: server.mem_percent },
    { label: 'Disk', value: server.disk_percent },
  ].filter((metric): metric is { label: string; value: number } => typeof metric.value === 'number');
  const worst = metrics.sort((a, b) => b.value - a.value)[0];
  return worst ? `${worst.label} ${Math.round(worst.value)}%` : '';
}

function mergeDays(checks: Schemas['UptimeCheck'][]) {
  const byDay = new Map<number, Schemas['UptimeDay']>();
  for (const day of checks.filter((check) => !check.paused).flatMap((check) => check.days)) {
    const merged = byDay.get(day.day) ?? { day: day.day, down: 0, slow: 0, total: 0 };
    byDay.set(day.day, {
      day: day.day,
      down: merged.down + day.down,
      slow: merged.slow + day.slow,
      total: merged.total + day.total,
    });
  }
  return [...byDay.values()].sort((a, b) => a.day - b.day);
}

function countCleanDays(days: Schemas['UptimeDay'][]) {
  let count = 0;
  for (const day of [...days].reverse()) {
    if (day.down > 0) {
      break;
    }
    count += 1;
  }
  return count;
}

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
    (alert) => !findMute(mutes, alert.rule_id, alert.server_id ?? alert.check_id ?? alert.cron_id)
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
      iconClassName: CRITICAL_ICON,
      title: `Incident open: ${incident.title}`,
      detail: `${getStatusMeta(incident.status).label} since ${formatClock(Date.parse(incident.started_at))}.`,
      href: `/incidents/${incident.id}`,
      action: 'Open Incident',
    })),
    ...activeAlerts.map((alert) => ({
      key: `alert-${alert.id}`,
      icon: alert.severity === 'critical' ? ('circle-x' as const) : ('triangle-alert' as const),
      iconClassName: alert.severity === 'critical' ? CRITICAL_ICON : WARNING_ICON,
      title: `${alert.rule_name} on ${alert.target_name}`,
      detail: `${describeFiring(alert)}.`,
      ...getAlertLink(alert),
    })),
    ...downServers
      .filter((server) => !alertedIds.has(server.id))
      .map((server) => ({
        key: `server-${server.id}`,
        icon: 'circle-x' as const,
        iconClassName: CRITICAL_ICON,
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
        iconClassName: check.status === 'down' ? CRITICAL_ICON : WARNING_ICON,
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
        iconClassName: cron.status === 'late' ? WARNING_ICON : CRITICAL_ICON,
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
                    <Icon name={item.icon} className={cn('shrink-0', item.iconClassName)} />
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

          <EventsTile events={events} />

          <div className="grid grid-cols-2 gap-4 max-lg:grid-cols-1">
            <Tile
              title="Servers"
              href="/servers"
              linkLabel={`View all ${servers.length}`}
              value={`${servers.length - downServers.length} of ${servers.length} up`}
              caption={
                trends && trends.cpu.length > 0
                  ? `avg CPU ${Math.round(trends.cpu.reduce((sum, point) => sum + point.value, 0) / trends.cpu.length)}%, last 24 hours`
                  : undefined
              }
              chart={
                <TrendBars
                  points={trends?.cpu ?? []}
                  label={(value) => `${Math.round(value)}% CPU`}
                  minScale={10}
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
        </div>
      </div>
    </div>
  );
}

type TileProps = {
  title: string;
  href: string;
  linkLabel: string;
  value: string;
  caption?: string;
  chart: ReactNode;
  healthy: ReactNode;
  children: ReactNode[] | ReactNode;
};

function Tile(props: TileProps) {
  const { title, href, linkLabel, value, caption, chart, healthy, children } = props;

  const rows = Array.isArray(children) ? children.filter(Boolean) : children ? [children] : [];

  return (
    <section className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default pb-1 shadow-card">
      <div className="flex flex-col gap-3 p-4">
        <div className="flex items-center gap-2">
          <h2 className="flex-1 text-title-section font-medium">{title}</h2>
          <Link
            to={href}
            className="text-body text-text-secondary outline-none hover:text-text-primary focus-visible:underline"
          >
            {linkLabel}
          </Link>
        </div>
        <p className="flex items-baseline gap-1.5 whitespace-nowrap">
          <span className="text-body font-semibold">{value}</span>
          {caption && <span className="truncate text-caption font-normal text-text-tertiary">{caption}</span>}
        </p>
        {chart}
      </div>
      <div className="flex flex-col [&>*]:border-t [&>*]:border-border-row">
        {rows.length > 0 ? (
          rows
        ) : (
          <div className="flex items-center gap-2.5 px-4 py-2.75">
            <Badge tone="success">
              <Icon name="check" />
              Healthy
            </Badge>
            <span className="truncate text-body text-text-secondary">{healthy}</span>
          </div>
        )}
      </div>
    </section>
  );
}

type TileRowProps = {
  href: string;
  badge: ReactNode;
  label: string;
  meta: string;
  isMono?: boolean;
  more?: number;
};

function TileRow(props: TileRowProps) {
  const { href, badge, label, meta, isMono, more = 0 } = props;

  return (
    <Link
      to={href}
      className="flex items-center gap-2.5 px-4 py-2.75 outline-none hover:bg-surface-subtle focus-visible:bg-surface-subtle"
    >
      {badge}
      <span className="flex min-w-0 flex-1 items-baseline gap-1.5">
        <span className={cn('truncate text-body', isMono && 'font-mono text-code')}>{label}</span>
        {more > 0 && <span className="shrink-0 text-caption font-normal text-text-tertiary">+{more} more</span>}
      </span>
      <span className="shrink-0 text-caption font-normal text-text-tertiary">{meta}</span>
    </Link>
  );
}

type TrendBarsProps = {
  points: Schemas['TrendPoint'][];
  label: (value: number) => string;
  minScale: number;
  fillClassName: string;
  warnAt?: number;
  criticalAt?: number;
};

const HOUR_MS = 3_600_000;
const TREND_HOURS = 24;

function TrendBars(props: TrendBarsProps) {
  const { points, label, minScale, fillClassName, warnAt, criticalAt } = props;

  const byHour = new Map(points.map((point) => [point.at, point.value]));
  const lastHour = Math.floor(Date.now() / HOUR_MS) * HOUR_MS;
  const hours = Array.from({ length: TREND_HOURS }, (_, index) => lastHour - (TREND_HOURS - 1 - index) * HOUR_MS);
  const top = Math.max(minScale, ...points.map((point) => point.value)) * 1.25;

  return (
    <div className="flex h-7 items-stretch gap-0.5">
      {hours.map((hour) => {
        const value = byHour.get(hour);
        return (
          <span
            key={hour}
            title={value === undefined ? `${formatClock(hour)}: no data` : `${formatClock(hour)}: ${label(value)}`}
            className="relative min-w-0.5 flex-1 overflow-hidden rounded-[1px] bg-surface-fill"
          >
            {value !== undefined && value > 0 && (
              <span
                className={cn(
                  'absolute inset-x-0 bottom-0',
                  criticalAt !== undefined && value >= criticalAt
                    ? 'bg-brand'
                    : warnAt !== undefined && value >= warnAt
                      ? 'bg-toast-warning-icon'
                      : fillClassName
                )}
                style={{ height: `${Math.max((value / top) * 100, 8)}%` }}
              />
            )}
          </span>
        );
      })}
    </div>
  );
}

type TelemetryTileProps = {
  issues: Schemas['Issue'][];
  errors: Schemas['TrendPoint'][];
  isSetUp: boolean;
};

function TelemetryTile(props: TelemetryTileProps) {
  const { issues, errors, isSetUp } = props;

  const newIssues = issues.filter((issue) => issue.first_seen_at / 1_000 > Date.now() - DAY_MS);
  const hourErrors = errors.find((point) => point.at === Math.floor(Date.now() / HOUR_MS) * HOUR_MS)?.value ?? 0;

  if (!isSetUp) {
    return (
      <Tile
        title="Telemetry"
        href="/traces/setup"
        linkLabel="Set Up"
        value="No traces yet"
        caption="send spans from any OpenTelemetry SDK"
        chart={<TrendBars points={[]} label={String} minScale={1} fillClassName="bg-brand" />}
        healthy=""
      >
        <TileRow
          href="/traces/setup"
          badge={
            <Badge>
              <Icon name="terminal" />
              Setup
            </Badge>
          }
          label="Set up traces to see errors and slow requests"
          meta=""
        />
      </Tile>
    );
  }

  return (
    <Tile
      title="Telemetry"
      href="/issues"
      linkLabel="View all issues"
      value={newIssues.length === 0 ? 'No new issues' : pluralize(newIssues.length, 'new issue')}
      caption={`${pluralize(hourErrors, 'error')} this hour`}
      chart={
        <TrendBars
          points={errors}
          label={(value) => pluralize(value, 'error')}
          minScale={4}
          fillClassName="bg-brand"
        />
      }
      healthy="No new errors today."
    >
      {newIssues.slice(0, 1).map((issue) => (
        <TileRow
          key={issue.fingerprint}
          more={newIssues.length - 1}
          href={`/issues/${issue.fingerprint}`}
          badge={
            <Badge>
              <span className="font-mono">{issue.service}</span>
            </Badge>
          }
          label={issue.kind}
          isMono
          meta={pluralize(issue.count, 'event')}
        />
      ))}
    </Tile>
  );
}
