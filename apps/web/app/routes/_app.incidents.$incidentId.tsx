import { useQuery } from '@tanstack/react-query';
import { Link, useParams } from 'react-router';

import { IncidentDetail } from '~/components/incidents/incident-detail';
import { Button } from '~/components/ui/button';
import { PageBar } from '~/components/ui/page-bar';
import { Skeleton } from '~/components/ui/skeleton';
import { incidentOptions } from '~/queries/incidents';

export function meta() {
  return [{ title: 'Incident · Daroyan' }];
}

export default function IncidentPage() {
  const { incidentId = '' } = useParams();
  const { data: incident, isLoading, isError } = useQuery(incidentOptions(incidentId));

  if (isLoading) {
    return (
      <div className="flex min-h-full flex-col bg-surface-default">
        <PageBar backTo="/incidents" icon="flag" title="Incidents" />
        <div className="flex justify-center px-10 pt-3 max-lg:px-4">
          <div className="flex w-200 max-w-full flex-col gap-3">
            <Skeleton className="h-8 w-80" />
            <Skeleton className="h-4 w-60" />
            <Skeleton className="mt-4 h-18 w-full" />
          </div>
        </div>
      </div>
    );
  }

  if (isError || !incident) {
    return (
      <div className="flex min-h-full flex-col bg-surface-default">
        <PageBar backTo="/incidents" icon="flag" title="Incidents" />
        <div className="flex flex-1 flex-col items-center justify-center gap-1 pb-20 text-center">
          <p className="text-title-section font-medium">Incident not found</p>
          <p className="text-body text-text-secondary">It may have been deleted.</p>
          <Button nativeButton={false} variant="secondary" className="mt-3 w-fit" render={<Link to="/incidents" />}>
            Back to Incidents
          </Button>
        </div>
      </div>
    );
  }

  return <IncidentDetail incident={incident} />;
}
