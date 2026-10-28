import { useQuery } from '@tanstack/react-query';

import { CronBody } from '~/components/crons/cron-body';
import { cronOptions } from '~/queries/crons';

import type { Route } from './+types/_app.crons.$cronId._index';

export function meta() {
  return [{ title: 'Cron · Daroyan' }];
}

export default function CronPage(props: Route.ComponentProps) {
  const { cronId } = props.params;

  const { data: cron, error } = useQuery(cronOptions(cronId));

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Cron Not Found</p>
        <p className="text-body text-text-secondary">It may have been deleted.</p>
      </div>
    );
  }

  return cron ? <CronBody cron={cron} /> : null;
}
