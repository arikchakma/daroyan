import { useQuery } from '@tanstack/react-query';
import { useEffect } from 'react';
import { useParams } from 'react-router';

import { PublicStatus } from '~/components/status-pages/public-status';
import { Skeleton } from '~/components/ui/skeleton';
import { ApiError } from '~/lib/http';
import { publicStatusPageOptions } from '~/queries/status-pages';

export function meta() {
  return [{ title: 'Status' }];
}

export default function PublicStatusRoute() {
  const { slug = '' } = useParams();
  const { data: page, error } = useQuery(publicStatusPageOptions(slug));

  useEffect(() => {
    if (page) {
      document.title = page.name;
    }
  }, [page?.name]);

  return (
    <div className="min-h-dvh bg-surface-page px-4 py-10 text-text-primary max-sm:py-6">
      {page ? (
        <PublicStatus slug={slug} page={page} />
      ) : error ? (
        <div className="mx-auto flex max-w-90 flex-col items-center gap-1.5 pt-20 text-center">
          <p className="text-title-section font-medium">
            {error instanceof ApiError && error.status === 404 ? 'Status Page Not Found' : 'Something Went Wrong'}
          </p>
          <p className="text-body text-text-secondary">
            {error instanceof ApiError && error.status === 404
              ? 'This status page does not exist or is not public.'
              : 'The status page could not load. Try again in a moment.'}
          </p>
        </div>
      ) : (
        <div className="mx-auto flex w-full max-w-190 flex-col gap-8">
          <Skeleton size="title" className="w-48" />
          <Skeleton size="block" className="h-18.5 rounded-2xl" />
          <Skeleton size="block" className="h-96 rounded-2xl" />
        </div>
      )}
    </div>
  );
}
