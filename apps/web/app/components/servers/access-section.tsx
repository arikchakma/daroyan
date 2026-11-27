import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router';

import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { DataTable } from '~/components/ui/data-table';
import { useAccessTable } from '~/hooks/use-access-table';
import { serverQueries } from '~/queries/servers';
import { getFailedLoginsText } from '~/utils/access';

type AccessSectionProps = {
  serverId: string;
};

export function AccessSection(props: AccessSectionProps) {
  const { serverId } = props;

  const { data } = useQuery(serverQueries.access(serverId));

  const events = data?.events ?? [];
  const table = useAccessTable(events);
  const logsUrl = `/logs?range=7d&server_id=${encodeURIComponent(serverId)}&service=access`;

  return (
    <DetailSection title="Access" summary={data ? getFailedLoginsText(data.failed_logins) : undefined}>
      <DetailCard>
        {data?.events.length !== 0 ? (
          <DataTable table={table} label="Access" variant="card" size="lg" isLoading={!data} />
        ) : (
          <DetailEmpty>
            No logins, sudo or account changes yet. The agent reads them from the journal unless{' '}
            <code className="font-mono text-code">agent.access_logs</code> is false.
          </DetailEmpty>
        )}
      </DetailCard>
      {events.length > 0 && (
        <Link to={logsUrl} className="self-start rounded-xs text-body text-text-primary outline-none hover:underline focus-visible:underline">
          Open Access Logs
        </Link>
      )}
    </DetailSection>
  );
}
