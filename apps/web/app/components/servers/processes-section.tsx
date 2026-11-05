import type { Schemas } from '@daroyan/client';

import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { DataTable } from '~/components/ui/data-table';
import { useProcessesTable } from '~/hooks/use-processes-table';
import { isProcessDegraded } from '~/utils/servers';

type ProcessesSectionProps = {
  serverId: string;
  processes: Schemas['ProcessState'][];
};

export function ProcessesSection(props: ProcessesSectionProps) {
  const { serverId, processes } = props;

  const table = useProcessesTable({ serverId, processes });
  const running = processes.filter((process) => process.status === 'running' && !isProcessDegraded(process)).length;
  const degraded = processes.filter(isProcessDegraded).length;
  const summary = degraded > 0 ? `${running} of ${processes.length} running, ${degraded} degraded` : `${running} of ${processes.length} running`;

  return (
    <DetailSection title="Processes" summary={processes.length > 0 ? summary : undefined}>
      <DetailCard>
        {processes.length > 0 ? (
          <DataTable table={table} label="Processes" variant="card" size="lg" />
        ) : (
          <DetailEmpty>
            No watched processes. Add one under <code className="font-mono text-code">processes</code> in the agent
            config.
          </DetailEmpty>
        )}
      </DetailCard>
    </DetailSection>
  );
}
