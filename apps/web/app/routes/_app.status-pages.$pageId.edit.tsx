import { useQuery } from '@tanstack/react-query';

import { StatusPageForm } from '~/components/status-pages/status-page-form';
import { statusPageOptions } from '~/queries/status-pages';

import type { Route } from './+types/_app.status-pages.$pageId.edit';

export function meta() {
  return [{ title: 'Edit Status Page · Daroyan' }];
}

export default function EditStatusPagePage(props: Route.ComponentProps) {
  const { pageId } = props.params;

  const { data: page, error } = useQuery(statusPageOptions(pageId));

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Status Page Not Found</p>
        <p className="text-body text-text-secondary">It may have been deleted.</p>
      </div>
    );
  }

  return page ? <StatusPageForm key={page.id} page={page} /> : null;
}
