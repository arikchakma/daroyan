import type { Schemas } from '@daroyan/client';

import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { DataTable } from '~/components/ui/data-table';
import { useFilesystemsTable } from '~/hooks/use-filesystems-table';
import { getPercent } from '~/utils/format';

type FilesystemsSectionProps = {
  disks: Schemas['DiskState'][];
};

export function FilesystemsSection(props: FilesystemsSectionProps) {
  const { disks } = props;

  const table = useFilesystemsTable(disks);
  const full = disks.filter((disk) => getPercent(disk.disk_used, disk.disk_total) >= 90).length;

  return (
    <DetailSection
      title="Filesystems"
      summary={disks.length > 0 ? (full > 0 ? `${full} almost full` : `${disks.length} OK`) : undefined}
    >
      <DetailCard>
        {disks.length > 0 ? (
          <DataTable table={table} label="Filesystems" variant="card" size="lg" />
        ) : (
          <DetailEmpty>The agent reported no filesystems.</DetailEmpty>
        )}
      </DetailCard>
    </DetailSection>
  );
}
