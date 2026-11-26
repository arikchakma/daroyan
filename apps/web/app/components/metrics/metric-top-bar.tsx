import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { PageBar } from '~/components/ui/page-bar';
import { getDefaultAggregation, getMetricTracesLink } from '~/utils/metrics';
import type { MetricAggregation, MetricSummary } from '~/utils/metrics';

type MetricTopBarProps = {
  metric?: MetricSummary;
  aggregation?: MetricAggregation;
  service?: string;
  filters?: string[];
  groupBy?: string;
};

export function MetricTopBar(props: MetricTopBarProps) {
  const { metric, aggregation, service, filters, groupBy } = props;

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
  filters?.forEach((filter) => alertParams.append('metricFilter', filter));
  if (groupBy) {
    alertParams.set('metricGroupBy', groupBy);
  }

  return (
    <PageBar backTo="/metrics" icon="chart-line" title="Metrics">
      <Button nativeButton={false} variant="secondary" render={<Link to={`/alerts/new?${alertParams}`} />} className="w-fit">
        Create Alert
      </Button>
      <Button nativeButton={false} render={<Link to={getMetricTracesLink(metric, service)} />} className="w-fit">
        <Icon name="layers" />
        View Traces
      </Button>
    </PageBar>
  );
}
