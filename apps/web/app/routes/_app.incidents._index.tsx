import { useQuery } from '@tanstack/react-query';

import { Icon } from '~/components/icons/icon';
import { DeclareIncidentSheet } from '~/components/incidents/declare-incident-sheet';
import { ScheduleMaintenanceSheet } from '~/components/incidents/schedule-maintenance-sheet';
import { Button } from '~/components/ui/button';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState, NoResults } from '~/components/ui/empty-state';
import { PageBar } from '~/components/ui/page-bar';
import { ViewsToolbar } from '~/components/ui/views-toolbar';
import { useIncidentsTable } from '~/hooks/use-incidents-table';
import { useViewParams } from '~/hooks/use-list-params';
import { incidentQueries } from '~/queries/incidents';
import { isClosed, sortIncidents } from '~/utils/incidents';
import type { Incident } from '~/utils/incidents';

type IncidentView = 'all' | 'ongoing' | 'scheduled' | 'resolved';

const VIEW_FILTERS: Record<IncidentView, (incident: Incident) => boolean> = {
  all: () => true,
  ongoing: (incident) => !isClosed(incident.status) && incident.status !== 'scheduled',
  scheduled: (incident) => incident.status === 'scheduled',
  resolved: (incident) => isClosed(incident.status),
};

const VIEW_VALUES = Object.keys(VIEW_FILTERS) as IncidentView[];

const VIEW_LABELS: Record<IncidentView, string> = {
  all: 'All',
  ongoing: 'Ongoing',
  scheduled: 'Scheduled',
  resolved: 'Resolved',
};

export function meta() {
  return [{ title: 'Incidents · Daroyan' }];
}

export default function IncidentsPage() {
  const { data: incidents, isLoading } = useQuery(incidentQueries.list());
  const { searchParams, search, view, setView, setSearch, setParam, reset, isFiltered } = useViewParams(VIEW_VALUES);
  const creating = searchParams.get('new');

  const all = incidents ?? [];
  const term = search.trim().toLowerCase();
  const visible = sortIncidents(
    all.filter(
      (incident) =>
        VIEW_FILTERS[view](incident) &&
        (!term ||
          incident.title.toLowerCase().includes(term) ||
          incident.components.some((component) => component.name.toLowerCase().includes(term)))
    )
  );
  const views = (Object.keys(VIEW_FILTERS) as IncidentView[]).map((value) => ({
    value,
    label: VIEW_LABELS[value],
    count: all.filter(VIEW_FILTERS[value]).length,
  }));

  const table = useIncidentsTable(visible);
  const isEmpty = incidents?.length === 0;

  const actions = (
    <>
      <Button variant="secondary" className="w-fit" onClick={() => setParam('new', 'maintenance')}>
        <Icon name="calendar" />
        Schedule Maintenance
      </Button>
      <Button className="w-fit" onClick={() => setParam('new', 'incident')}>
        <Icon name="plus" />
        Declare Incident
      </Button>
    </>
  );

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="flag" title="Incidents">
        {!isEmpty && actions}
      </PageBar>

      {isEmpty ? (
        <EmptyState
          icon="flag"
          marks={['check', 'check', 'clock']}
          title="No incidents yet"
          description="Declare one when something breaks, so your team and status pages know. Plan downtime ahead with maintenance."
        >
          {actions}
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-2 px-3 pt-1 pb-3">
          <ViewsToolbar
            views={views}
            view={view}
            onViewChange={setView}
            searchLabel="Search incidents and maintenance"
            search={search}
            onSearchChange={setSearch}
            isViewChanged={isFiltered}
            onReset={reset}
          />
          <DataTable table={table} label="Incidents" size="lg" isLoading={isLoading} />
          {incidents && visible.length === 0 && (
            <NoResults title="No incidents found" onReset={reset} />
          )}
        </div>
      )}

      <DeclareIncidentSheet
        open={creating === 'incident'}
        onOpenChange={(open) => setParam('new', open ? 'incident' : null)}
      />
      <ScheduleMaintenanceSheet
        open={creating === 'maintenance'}
        onOpenChange={(open) => setParam('new', open ? 'maintenance' : null)}
      />
    </div>
  );
}
