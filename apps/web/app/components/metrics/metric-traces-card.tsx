import { MetricSideCard } from '~/components/metrics/metric-side-card';
import { MetricSideRow } from '~/components/metrics/metric-side-row';
import type { MetricResult } from '~/utils/metrics';
import { formatMicros, getTraceLink } from '~/utils/traces';

const VISIBLE_TRACES = 4;

type MetricTracesCardProps = {
  exemplars?: MetricResult['exemplars'];
  tracesLink: string;
};

export function MetricTracesCard(props: MetricTracesCardProps) {
  const { exemplars = [], tracesLink } = props;

  const slowest = [...new Map(exemplars.map((exemplar) => [exemplar.trace_id, exemplar])).values()]
    .filter((exemplar) => exemplar.duration_us !== null && exemplar.duration_us !== undefined)
    .sort((a, b) => (b.duration_us ?? 0) - (a.duration_us ?? 0))
    .slice(0, VISIBLE_TRACES);

  return (
    <MetricSideCard
      title="Slowest Requests"
      linkLabel="View Traces"
      linkTo={tracesLink}
      empty={slowest.length === 0 ? 'No sampled requests in this range.' : undefined}
    >
      {slowest.map((exemplar) => (
        <MetricSideRow
          key={exemplar.trace_id}
          to={getTraceLink(exemplar.trace_id, exemplar.at * 1_000)}
          label={exemplar.span_name ?? exemplar.group ?? 'Request'}
          detail={formatMicros(exemplar.duration_us)}
        />
      ))}
    </MetricSideCard>
  );
}
