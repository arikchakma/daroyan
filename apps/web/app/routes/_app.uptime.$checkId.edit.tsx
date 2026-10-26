import { useQuery } from '@tanstack/react-query';

import { CheckForm } from '~/components/uptime/check-form';
import { uptimeCheckOptions } from '~/queries/uptime';

import type { Route } from './+types/_app.uptime.$checkId.edit';

export function meta() {
  return [{ title: 'Edit Check · Daroyan' }];
}

export default function EditCheckPage(props: Route.ComponentProps) {
  const { checkId } = props.params;

  const { data: check, error } = useQuery({ ...uptimeCheckOptions(checkId), refetchInterval: false });

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Check Not Found</p>
        <p className="text-body text-text-secondary">It may have been deleted.</p>
      </div>
    );
  }

  return check ? <CheckForm key={check.id} check={check} /> : null;
}
