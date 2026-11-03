import { useQuery } from '@tanstack/react-query';
import { useId } from 'react';

import { SettingsBlock, SettingsCard, SettingsPage } from '~/components/settings/settings-page';
import { SettingsRow } from '~/components/settings/settings-row';
import { ThemeSelect } from '~/components/settings/theme-select';
import { Skeleton } from '~/components/ui/skeleton';
import { authQueries } from '~/queries/auth';
import { instanceQueries } from '~/queries/instance';
import { pluralize } from '~/utils/format';
import { getRoleLabel } from '~/utils/settings';

export function meta() {
  return [{ title: 'General · Daroyan' }];
}

export default function GeneralSettingsPage() {
  const themeLabelId = useId();
  const { data: me } = useQuery(authQueries.me());
  const { data: instance } = useQuery(instanceQueries.detail());

  return (
    <SettingsPage title="General" icon="settings">
      <SettingsBlock title="Account" description="You sign in with Google. Theme only changes this browser.">
        <SettingsCard className="divide-y divide-border-row">
          <SettingsRow label="Name">{me?.name ?? <Skeleton className="w-28" />}</SettingsRow>
          <SettingsRow label="Email">{me?.email ?? <Skeleton className="w-40" />}</SettingsRow>
          <SettingsRow label="Role">{me ? getRoleLabel(me.role) : <Skeleton className="w-16" />}</SettingsRow>
          <SettingsRow label="Theme" labelId={themeLabelId}>
            <ThemeSelect labelId={themeLabelId} />
          </SettingsRow>
        </SettingsCard>
      </SettingsBlock>

      <SettingsBlock
        title="This Instance"
        description="These come from the API environment. Change one and restart the API to apply it."
      >
        <SettingsCard className="divide-y divide-border-row">
          <SettingsRow label="Version">{__DAROYAN_VERSION__}</SettingsRow>
          <SettingsRow label="Public URL" envVar="APP_URL">
            {instance ? (instance.app_url ?? 'Not set') : <Skeleton className="w-40" />}
          </SettingsRow>
          <SettingsRow label="Metrics and Events" envVar="RETENTION_DAYS">
            {instance ? `Kept ${pluralize(instance.retention_days, 'day')}` : <Skeleton className="w-20" />}
          </SettingsRow>
          <SettingsRow label="Logs" envVar="LOG_RETENTION_DAYS">
            {instance ? `Kept ${pluralize(instance.log_retention_days, 'day')}` : <Skeleton className="w-20" />}
          </SettingsRow>
        </SettingsCard>
      </SettingsBlock>
    </SettingsPage>
  );
}
