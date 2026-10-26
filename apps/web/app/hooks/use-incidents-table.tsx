import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { IncidentStatusBadge, SeverityBadge } from '~/components/incidents/incident-badges';
import { Badge } from '~/components/ui/badge';
import { Chip } from '~/components/ui/chip';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import {
  describeShownPages,
  formatCountdown,
  formatIncidentDuration,
  formatMoment,
  formatWindow,
  isClosed,
  isMaintenance,
} from '~/utils/incidents';
import type { Incident } from '~/utils/incidents';

const helper = createColumnHelper<DataTableFeatures, Incident>();

function describeIncident(incident: Incident) {
  const pages = describeShownPages(incident);
  if (isMaintenance(incident)) {
    return pages === 'internal only' ? 'Maintenance' : `Maintenance · ${pages}`;
  }
  const lead = incident.lead ? `Lead: ${incident.lead.name}` : 'No lead';
  if (incident.postmortem_published_at) {
    return `${lead} · postmortem published`;
  }
  return isClosed(incident.status) ? lead : `${lead} · ${pages}`;
}

function needsPostmortem(incident: Incident) {
  return !isMaintenance(incident) && isClosed(incident.status) && !incident.postmortem_published_at;
}

export function useIncidentsTable(incidents: Incident[]) {
  const columns = useMemo(
    () =>
      helper.columns([
        helper.display({
          id: 'incident',
          header: 'Incident',
          meta: { skeleton: 'lines' },
          cell: ({ row }) => (
            <span className="flex min-w-0 items-center gap-2.5">
              <span className="flex min-w-0 flex-col gap-0.5">
                <Link
                  to={`/incidents/${row.original.id}`}
                  className="truncate font-medium outline-none after:absolute after:inset-0 focus-visible:underline"
                >
                  {row.original.title}
                </Link>
                <span className="truncate text-small text-text-tertiary">{describeIncident(row.original)}</span>
              </span>
              {needsPostmortem(row.original) && (
                <Badge tone="attention" className="max-lg:hidden">
                  <Icon name="file-text" />
                  No Postmortem
                </Badge>
              )}
            </span>
          ),
        }),
        helper.display({
          id: 'status',
          header: 'Status',
          meta: { width: 140, skeleton: 'badge' },
          cell: ({ row }) => <IncidentStatusBadge status={row.original.status} />,
        }),
        helper.display({
          id: 'severity',
          header: 'Severity',
          meta: { width: 100, skeleton: 'badge', hideBelow: 'sm' },
          cell: ({ row }) => <SeverityBadge severity={row.original.severity} />,
        }),
        helper.display({
          id: 'affects',
          header: 'Affects',
          meta: { width: 240, hideBelow: 'lg' },
          cell: ({ row }) => (
            <span className="flex min-w-0 items-center gap-1.5 overflow-hidden">
              {row.original.components.map((component) => (
                <Chip key={`${component.kind}-${component.id}`}>{component.name}</Chip>
              ))}
            </span>
          ),
        }),
        helper.display({
          id: 'duration',
          header: 'Duration',
          meta: { width: 160, align: 'right', hideBelow: 'md' },
          cell: ({ row }) => {
            const incident = row.original;
            if (isMaintenance(incident) && incident.starts_at && incident.ends_at) {
              return formatWindow(incident.starts_at, incident.ends_at);
            }
            return formatIncidentDuration(incident);
          },
        }),
        helper.display({
          id: 'started',
          header: 'Started',
          meta: { width: 120, align: 'right', hideBelow: 'md' },
          cell: ({ row }) => {
            const incident = row.original;
            if (incident.status === 'scheduled' && incident.starts_at) {
              const countdown = formatCountdown(incident.starts_at);
              return countdown.charAt(0).toUpperCase() + countdown.slice(1);
            }
            return formatMoment(incident.started_at);
          },
        }),
      ]),
    []
  );

  return useDataTable({ columns, data: incidents });
}
