import { useQuery } from '@tanstack/react-query';
import { useParams, useSearchParams } from 'react-router';

import { TraceDetail } from '~/components/traces/trace-detail';
import { traceQueries } from '~/queries/traces';
import { orderSpans } from '~/utils/traces';

export function meta() {
  return [{ title: 'Trace · Daroyan' }];
}

export default function TracePage() {
  const { traceId = '' } = useParams();
  const [searchParams] = useSearchParams();
  const at = Number(searchParams.get('at')) || undefined;
  const { data: spans, isError } = useQuery(traceQueries.detail(traceId, at));

  if (isError || spans?.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Trace Not Found</p>
        <p className="text-body text-text-secondary">It may be older than the retention window.</p>
      </div>
    );
  }

  if (!spans) {
    return null;
  }

  return <TraceDetail traceId={traceId} nodes={orderSpans(spans)} />;
}
