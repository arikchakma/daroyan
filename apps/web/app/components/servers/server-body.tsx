import type { Schemas } from '@daroyan/client';

import { AlertBadge, AlertBanners } from '~/components/alerts/alert-banners';
import { Icon } from '~/components/icons/icon';
import { EventsCard } from '~/components/servers/events-card';
import { FilesystemsSection } from '~/components/servers/filesystems-section';
import { MetricsSection } from '~/components/servers/metrics-section';
import { PortsSection } from '~/components/servers/ports-section';
import { ProcessesSection } from '~/components/servers/processes-section';
import { RestartProcessDialog } from '~/components/servers/restart-process-dialog';
import { ServerStatusBadge } from '~/components/servers/server-status-badge';
import { SpecGrid } from '~/components/servers/spec-grid';
import { formatRelative } from '~/utils/format';

type ServerBodyProps = {
  server: Schemas['ServerDetail'];
};

export function ServerBody(props: ServerBodyProps) {
  const { server } = props;

  const host = server.host;
  const report = server.last_report;
  const subtitle = [host?.hostname, 'reports every 10s'].filter(Boolean).join(' · ');

  return (
    <div className="flex flex-1 justify-center px-10 pt-3 pb-12">
      <div className="flex w-full max-w-240 min-w-0 flex-col gap-6">
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2.5">
            <h1 className="truncate text-title-page font-semibold text-text-primary">{server.name}</h1>
            {server.status === 'down' ? (
              <ServerStatusBadge status={server.status} />
            ) : (
              <AlertBadge serverId={server.id} fallback={<ServerStatusBadge status={server.status} />} />
            )}
          </div>
          <div className="flex flex-wrap items-center gap-2.5">
            <p className="text-body text-text-secondary">{subtitle}</p>
            {server.labels.length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {server.labels.map((label) => (
                  <span
                    key={label}
                    className="flex h-5.5 items-center rounded-xs bg-surface-fill px-1.75 text-caption font-normal text-text-secondary"
                  >
                    {label}
                  </span>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="flex flex-col gap-4 lg:flex-row">
          {host && report ? (
            <SpecGrid serverId={server.id} host={host} report={report} />
          ) : (
            <WaitingNotice />
          )}
          <EventsCard serverId={server.id} serverName={server.name} />
        </div>

        {server.status === 'down' && (
          <div className="flex items-start gap-3 rounded-xl border border-tone-critical-bg bg-tone-critical-surface px-4 py-3.5">
            <Icon name="triangle-alert" className="mt-0.5 text-tone-critical-fg" />
            <div className="flex flex-col gap-0.5">
              <p className="text-body font-medium text-tone-critical-fg">{server.name} is not connected</p>
              <p className="text-body text-text-primary">
                The last report came {server.last_seen_at ? formatRelative(server.last_seen_at) : 'never'}. Check the
                agent on the server with <code className="font-mono text-code">systemctl status daroyan</code>.
              </p>
            </div>
          </div>
        )}

        <AlertBanners serverId={server.id} />
        <RestartProcessDialog server={server} />

        {host && <MetricsSection serverId={server.id} host={host} report={report} />}
        {report && <ProcessesSection serverId={server.id} processes={report.processes} />}
        {report && <FilesystemsSection disks={report.state.disks} />}
        {report && <PortsSection ports={report.ports} />}
      </div>
    </div>
  );
}

function WaitingNotice() {
  return (
    <div className="flex min-w-0 flex-1 items-center gap-3 rounded-2xl border border-border-ring bg-surface-default px-4 py-3.5 shadow-card">
      <Icon name="loader" className="animate-spin text-icon-secondary" />
      <div className="flex flex-1 flex-col gap-0.5">
        <p className="text-body font-medium text-text-primary">Waiting for the first report</p>
        <p className="text-body text-text-secondary">The agent sends one every 10 seconds once it connects.</p>
      </div>
    </div>
  );
}
