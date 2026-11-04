import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { ComingUp } from '~/components/overview/coming-up';
import { EventsTile } from '~/components/overview/events-tile';
import { OverviewRow } from '~/components/overview/overview-row';
import { DetailSection } from '~/components/servers/detail-table';
import { Button } from '~/components/ui/button';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { useFiringAlerts } from '~/hooks/use-firing-alerts';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { incidentQueries } from '~/queries/incidents';
import { overviewQueries } from '~/queries/overview';
import { traceQueries } from '~/queries/traces';
import { describeFiring } from '~/utils/alerts';
import { formatUntil } from '~/utils/crons';
import { DAY_MS, formatClock, formatRelative, joinNames, pluralize } from '~/utils/format';
import { getStatusMeta, isClosed, isMaintenance } from '~/utils/incidents';
import { getAlertHref } from '~/utils/overview';
import { formatMs } from '~/utils/uptime';

const BAD_CRON_STATUSES = ['failed', 'missed', 'late'];

type OverviewDashboardProps = {
  servers: Schemas['ServerItem'][];
  checks: Schemas['UptimeCheck'][];
  crons: Schemas['Cron'][];
};

type AttentionItem = {
  key: string;
  icon: IconName;
  tone: 'critical' | 'warning';
  title: string;
  detail: string;
  href: string;
};

export function OverviewDashboard(props: OverviewDashboardProps) {
  const { servers, checks, crons } = props;

  const isAdmin = useIsAdmin();
  const { active: activeAlerts } = useFiringAlerts();
  const { data: incidents = [] } = useQuery(incidentQueries.list());
  const { data: events } = useQuery(overviewQueries.events());
  const { data: upcoming } = useQuery(overviewQueries.upcoming());
  const { data: services = [] } = useQuery(traceQueries.services(DAY_MS));

  const alertedIds = new Set(activeAlerts.flatMap((alert) => [alert.server_id, alert.check_id, alert.cron_id]));
  const openIncidents = incidents.filter((incident) => !isMaintenance(incident) && !isClosed(incident.status));
  const downServers = servers.filter((server) => server.status === 'down');
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
    })),
    ...activeAlerts.map((alert) => ({
      key: `alert-${alert.id}`,
      icon: alert.severity === 'critical' ? ('circle-x' as const) : ('triangle-alert' as const),
      tone: alert.severity === 'critical' ? ('critical' as const) : ('warning' as const),
      title: `${alert.rule_name} on ${alert.target_name}`,
      detail: `${describeFiring(alert)}.`,
      href: getAlertHref(alert),
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
      })),
  ];

  const scope = joinNames(
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

          {attention.length > 0 && (
            <DetailSection title="Needs Attention" summary={`${attention.length} open`}>
              <div className="overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
                {attention.map((item) => (
                  <OverviewRow
                    key={item.key}
                    href={item.href}
                    icon={item.icon}
                    tone={item.tone}
                    title={item.title}
                    detail={item.detail}
                  />
                ))}
              </div>
            </DetailSection>
          )}

          {upcoming && upcoming.items.length > 0 && <ComingUp items={upcoming.items} />}

          <EventsTile events={events} />
        </div>
      </div>
    </div>
  );
}
