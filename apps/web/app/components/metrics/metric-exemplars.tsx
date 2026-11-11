import { Link } from 'react-router';

import type { ChartSeries } from '~/components/servers/metric-chart';
import { formatClock } from '~/utils/format';
import { getGroupLabel } from '~/utils/metrics';
import type { MetricResult } from '~/utils/metrics';
import { getTraceLink } from '~/utils/traces';

type MetricExemplarsProps = {
  exemplars: MetricResult['exemplars'];
  series: ChartSeries[];
  bucketMs: number;
  from: number;
  to: number;
};

export function MetricExemplars(props: MetricExemplarsProps) {
  const { exemplars, series, bucketMs, from, to } = props;

  const shown = [...new Map(exemplars.map((exemplar) => [Math.floor(exemplar.at / bucketMs), exemplar])).values()];

  if (shown.length === 0) {
    return null;
  }

  return (
    <div role="group" aria-label="Sample traces" className="border-t border-border-ring pt-2.5">
      <div className="relative h-3">
        {shown.map((exemplar) => (
          <Link
            key={`${exemplar.trace_id}-${exemplar.at}`}
            to={getTraceLink(exemplar.trace_id, exemplar.at * 1_000)}
            aria-label={`Open trace from ${formatClock(exemplar.at)}, ${getGroupLabel(exemplar.group)}`}
            title={`${formatClock(exemplar.at)} · ${getGroupLabel(exemplar.group)}`}
            className="absolute top-1/2 size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-surface-default outline-none hover:scale-125 focus-visible:ring-2 focus-visible:ring-text-link/40"
            style={{
              left: `${((exemplar.at - from) / Math.max(to - from, 1)) * 100}%`,
              backgroundColor:
                series.find((line) => line.label === getGroupLabel(exemplar.group))?.color ??
                'var(--color-text-tertiary)',
            }}
          />
        ))}
      </div>
    </div>
  );
}
