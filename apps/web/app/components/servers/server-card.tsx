import type { Schemas } from '@daroyan/client';
import { Link } from 'react-router';

import { AlertBadge } from '~/components/alerts/alert-banners';
import { ServerCardStatus } from '~/components/servers/server-card-status';
import { ServerStatusBadge } from '~/components/servers/server-status-badge';
import { Spec } from '~/components/servers/spec';
import type { SpecProps } from '~/components/servers/spec';
import { formatBytes, formatCoresOf, formatPercent, formatUptimeDays, formatUsedOf } from '~/utils/format';
import { getUsageTone } from '~/utils/usage';

type ServerItem = Schemas['ServerItem'];

function getMeta(server: ServerItem) {
  const isLive = server.status === 'healthy' || server.status === 'warning';

  return [
    server.private_ip,
    [server.platform, server.platform_version].filter(Boolean).join(' '),
    isLive && server.uptime ? `up ${formatUptimeDays(server.uptime)}` : null,
  ]
    .filter(Boolean)
    .join(' · ');
}

function getUsageSpec(label: string, percent: number | null | undefined, detail: string): SpecProps {
  if (percent === null || percent === undefined) {
    return { label, value: '—', detail };
  }

  const tone = getUsageTone(percent);

  return { label, value: formatPercent(percent), detail, ring: { percent, tone }, valueCritical: tone === 'crit' };
}

function getSpecs(server: ServerItem): SpecProps[] {
  const cores = server.cpu_cores ?? 0;
  const memTotal = server.mem_total ?? 0;
  const diskTotal = server.disk_total ?? 0;

  if (server.status === 'down') {
    return [
      getUsageSpec('CPU', null, `${cores} vCPU`),
      getUsageSpec('Memory', null, formatBytes(memTotal)),
      getUsageSpec('Disk', null, formatBytes(diskTotal)),
    ];
  }

  return [
    getUsageSpec('CPU', server.cpu, formatCoresOf(server.cpu ?? 0, cores)),
    getUsageSpec('Memory', server.mem_percent, formatUsedOf(server.mem_used ?? 0, memTotal)),
    getUsageSpec('Disk', server.disk_percent, formatUsedOf(server.disk_used ?? 0, diskTotal)),
  ];
}

type ServerCardProps = {
  server: ServerItem;
};

export function ServerCard(props: ServerCardProps) {
  const { server } = props;

  const meta = getMeta(server);
  const isPending = server.status === 'pending' || !server.cpu_cores;

  return (
    <div className="relative flex min-w-0 flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card">
      <div className="flex min-w-0 flex-1 flex-col gap-1.5 px-4 py-3.5">
        <div className="flex min-w-0 items-center gap-2">
          <Link
            to={`/servers/${server.id}`}
            className="truncate text-body font-medium text-text-primary outline-none after:absolute after:inset-0 focus-visible:underline"
          >
            {server.name}
          </Link>
          {server.status === 'down' ? (
            <ServerStatusBadge status={server.status} />
          ) : (
            <AlertBadge serverId={server.id} fallback={<ServerStatusBadge status={server.status} />} />
          )}
          {server.labels.length > 0 && (
            <div className="ml-auto flex min-w-0 shrink gap-1.5 overflow-hidden">
              {server.labels.map((label) => (
                <span
                  key={label}
                  className="flex h-5.5 shrink-0 items-center rounded-xs bg-surface-fill px-1.75 text-caption font-normal text-text-secondary"
                >
                  {label}
                </span>
              ))}
            </div>
          )}
        </div>
        {meta && <p className="truncate text-body text-text-secondary">{meta}</p>}
        <ServerCardStatus server={server} />
      </div>

      {isPending ? (
        <p className="border-t border-border-row px-4 py-3.5 text-body text-text-tertiary">
          Run the install command on the server. CPU, memory and disk show up after the first report.
        </p>
      ) : (
        <div className="grid grid-cols-3 gap-px border-t border-border-row bg-border-row">
          {getSpecs(server).map((spec) => (
            <Spec key={spec.label} {...spec} />
          ))}
        </div>
      )}
    </div>
  );
}
