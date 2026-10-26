import { useMutation, useQuery } from '@tanstack/react-query';

import { Icon } from '~/components/icons/icon';
import { SettingsBlock, SettingsCard, SettingsPage } from '~/components/settings/settings-page';
import { Button } from '~/components/ui/button';
import { Skeleton } from '~/components/ui/skeleton';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { backupStatusOptions, createBackupOptions } from '~/queries/backups';
import { formatBytes, formatRelative } from '~/utils/format';

export function meta() {
  return [{ title: 'Backups · Daroyan' }];
}

export default function BackupsPage() {
  const isAdmin = useIsAdmin();
  const { data: status, isLoading } = useQuery(backupStatusOptions());
  const backUp = useMutation({
    ...createBackupOptions(),
    meta: { errorMessage: 'The backup failed' },
    onSuccess: (backup) => {
      queryClient.invalidateQueries({ queryKey: ['backups'] });
      toast.success(`Backed up ${formatBytes(backup.size_bytes)}`);
    },
  });

  const lastBackup = status?.last_backup;

  return (
    <SettingsPage title="Backups" icon="database">
      <SettingsBlock
        title="Metrics History"
        description="The API copies the DuckDB metrics and events every 6 hours, and uploads each copy to R2. Server settings and alert rules are in SQLite, which Litestream backs up all the time."
      >
        <SettingsCard className="flex-row items-center gap-3 p-4">
          <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-surface-fill">
            <Icon name="database" />
          </span>
          <div className="flex min-w-0 flex-1 flex-col text-body">
            {isLoading && (
              <>
                <Skeleton className="w-40" />
                <Skeleton className="w-64" />
              </>
            )}
            {status && !status.is_enabled && (
              <>
                <span className="font-medium">Backups are off</span>
                <span className="text-text-secondary">Set DUCKDB_BACKUP_DIR on the API to turn them on.</span>
              </>
            )}
            {status?.is_enabled && (
              <>
                <span className="font-medium">
                  {lastBackup ? `Last backup ${formatRelative(lastBackup.created_at)}` : 'No backup since the API started'}
                </span>
                <span className="truncate text-text-secondary">
                  {lastBackup ? `${lastBackup.name} · ${formatBytes(lastBackup.size_bytes)}` : 'The next scheduled backup runs within 6 hours.'}
                </span>
              </>
            )}
          </div>
          {isAdmin && (
            <Button
              type="button"
              variant="secondary"
              className="w-fit"
              disabled={!status?.is_enabled || backUp.isPending}
              onClick={() => backUp.mutate()}
            >
              {backUp.isPending && <Icon name="loader" className="animate-spin" />}
              Back Up Now
            </Button>
          )}
        </SettingsCard>
      </SettingsBlock>
    </SettingsPage>
  );
}
