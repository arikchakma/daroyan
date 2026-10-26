import { useQuery } from '@tanstack/react-query';

import { CronForm } from '~/components/crons/cron-form';
import { cronOptions } from '~/queries/crons';

import type { Route } from './+types/_app.crons.$cronId.edit';

export function meta() {
  return [{ title: 'Edit Cron · Daroyan' }];
}

export default function EditCronPage(props: Route.ComponentProps) {
  const { cronId } = props.params;

  const { data: cron, error } = useQuery({ ...cronOptions(cronId), refetchInterval: false });

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Cron Not Found</p>
        <p className="text-body text-text-secondary">It may have been deleted.</p>
      </div>
    );
  }

  return cron ? <CronForm key={cron.id} cron={cron} /> : null;
}
