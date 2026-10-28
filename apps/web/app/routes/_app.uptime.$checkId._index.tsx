import { useQuery } from '@tanstack/react-query';

import { CheckBody } from '~/components/uptime/check-body';
import { uptimeCheckOptions } from '~/queries/uptime';

import type { Route } from './+types/_app.uptime.$checkId._index';

export function meta() {
  return [{ title: 'Uptime Check · Daroyan' }];
}

export default function UptimeCheckPage(props: Route.ComponentProps) {
  const { checkId } = props.params;

  const { data: check, error } = useQuery(uptimeCheckOptions(checkId));

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Check Not Found</p>
        <p className="text-body text-text-secondary">It may have been deleted.</p>
      </div>
    );
  }

  return check ? <CheckBody check={check} /> : null;
}
