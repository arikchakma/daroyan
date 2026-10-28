import type { Schemas } from '@daroyan/client';

import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { DataTable } from '~/components/ui/data-table';
import { usePortsTable } from '~/hooks/use-ports-table';

type PortsSectionProps = {
  ports: Schemas['PortState'][];
};

export function PortsSection(props: PortsSectionProps) {
  const { ports } = props;

  const table = usePortsTable(ports);
  const open = ports.filter((port) => port.open).length;

  return (
    <DetailSection title="Ports and Hosts" summary={ports.length > 0 ? `${open} open` : undefined}>
      <DetailCard>
        {ports.length > 0 ? (
          <DataTable table={table} label="Ports and Hosts" variant="card" size="lg" />
        ) : (
          <DetailEmpty>
            No watched ports. Add one under <code className="font-mono text-code">ports</code> in the agent config.
          </DetailEmpty>
        )}
      </DetailCard>
    </DetailSection>
  );
}
