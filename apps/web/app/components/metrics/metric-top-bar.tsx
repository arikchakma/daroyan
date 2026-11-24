import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { PageBar } from '~/components/ui/page-bar';
import { getDefaultAggregation } from '~/utils/metrics';
import type { MetricAggregation, MetricSummary } from '~/utils/metrics';

type MetricTopBarProps = {
  metric?: MetricSummary;
  aggregation?: MetricAggregation;
  service?: string;
  filter?: string;
  groupBy?: string;
};

export function MetricTopBar(props: MetricTopBarProps) {
  const { metric, aggregation, service, filter, groupBy } = props;

  if (!metric) {
    return <PageBar backTo="/metrics" icon="chart-line" title="Metrics" />;
  }

  const alertParams = new URLSearchParams({ metricName: metric.name });
  if (aggregation && aggregation !== getDefaultAggregation(metric)) {
    alertParams.set('metricAggregation', aggregation);
  }
  if (service) {
    alertParams.set('metricService', service);
  }
  if (filter) {
    alertParams.set('metricFilter', filter);
  }
  if (groupBy) {
    alertParams.set('metricGroupBy', groupBy);
  }
  const traceService = service ?? (metric.services.length === 1 ? metric.services[0] : undefined);
  const tracesLink = traceService ? `/traces?${new URLSearchParams({ service: traceService })}` : '/traces';

  return (
    <PageBar backTo="/metrics" icon="chart-line" title="Metrics">
      <Button nativeButton={false} variant="secondary" render={<Link to={`/alerts/new?${alertParams}`} />} className="w-fit">
        Create Alert
      </Button>
      <Button nativeButton={false} render={<Link to={tracesLink} />} className="w-fit">
        <Icon name="layers" />
        View Traces
      </Button>
    </PageBar>
  );
}
