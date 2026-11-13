import { useQuery } from '@tanstack/react-query';
import { useSearchParams } from 'react-router';

import { AlertBadge, AlertBanners } from '~/components/alerts/alert-banners';
import { MetricOverTime } from '~/components/metrics/metric-over-time';
import { MetricRulesCard } from '~/components/metrics/metric-rules-card';
import { MetricSeriesSection } from '~/components/metrics/metric-series-section';
import { MetricSpecGrid } from '~/components/metrics/metric-spec-grid';
import { MetricTopBar } from '~/components/metrics/metric-top-bar';
import { MetricTracesCard } from '~/components/metrics/metric-traces-card';
import type { ChartSeries } from '~/components/servers/metric-chart';
import { useFiringAlerts } from '~/hooks/use-firing-alerts';
import { useMetricRules } from '~/hooks/use-metric-rules';
import { metricQueries } from '~/queries/metrics';
import { readMetricCondition } from '~/utils/alerts';
import {
  getAggregations,
  getGroupLabel,
  getDefaultAggregation,
  getKindLabel,
  getMetricUnitLabel,
  getSeriesColor,
  readMetricFilters,
  readMetricRange,
  writeMetricFilters,
} from '~/utils/metrics';

import type { Route } from './+types/_app.metrics.$name';

export function meta(props: Route.MetaArgs) {
  return [{ title: `${props.params.name} · Metrics · Daroyan` }];
}

export default function MetricPage(props: Route.ComponentProps) {
  const { name } = props.params;

  const [searchParams, setSearchParams] = useSearchParams();
  const range = readMetricRange(searchParams.get('range'));
  const service = searchParams.get('service') || undefined;
  const serverId = searchParams.get('server') || undefined;
  const groupBy = searchParams.get('group') || undefined;
  const filters = readMetricFilters(searchParams.get('filter') ?? '');
  const filter = writeMetricFilters(filters) || undefined;

  const { data: metrics, isSuccess } = useQuery(metricQueries.list());
  const metric = metrics?.find((item) => item.name === name);
  const aggregations = metric ? getAggregations(metric) : [];
  const aggregation =
    aggregations.find((item) => item.value === searchParams.get('agg'))?.value ?? (metric ? getDefaultAggregation(metric) : undefined);

  const { data: attributes } = useQuery({ ...metricQueries.attributes(name), enabled: !!metric });
  const { data: result } = useQuery({
    ...metricQueries.query({
      name,
      rangeMs: range.ms,
      agg: aggregation,
      service,
      server_id: serverId,
      filter,
      group_by: groupBy,
    }),
    enabled: !!metric,
  });

  const { rules, ruleIds } = useMetricRules(name);
  const { active } = useFiringAlerts({ ruleIds });
  const firingRuleIds = active.map((alert) => alert.rule_id);
  const limitRule =
    rules.find((rule) => firingRuleIds.includes(rule.id) && readMetricCondition(rule.condition)?.aggregation === aggregation) ??
    rules.find((rule) => (readMetricCondition(rule.condition)?.aggregation ?? (metric && getDefaultAggregation(metric))) === aggregation);
  const limit = limitRule ? readMetricCondition(limitRule.condition)?.threshold : undefined;

  const shown = result?.name === name ? result : undefined;
  const series: ChartSeries[] = (shown?.series ?? []).map((line, index) => ({
    label: getGroupLabel(line.group),
    values: line.values,
    color: getSeriesColor(index),
  }));

  const setParams = (changes: Record<string, string | null>) =>
    setSearchParams(
      (params) => {
        for (const [key, value] of Object.entries(changes)) {
          if (value) {
            params.set(key, value);
          } else {
            params.delete(key);
          }
        }
        return params;
      },
      { replace: true }
    );

  if (isSuccess && !metric) {
    return (
      <div className="flex h-full flex-col bg-surface-page">
        <MetricTopBar />
        <div className="flex flex-1 flex-col items-center justify-center gap-1 pb-20">
          <p className="text-title-section font-medium text-text-primary">Metric Not Found</p>
          <p className="text-body text-text-secondary">Nothing has reported {name} in the last 30 days.</p>
        </div>
      </div>
    );
  }

  const traceService = service ?? (metric?.services.length === 1 ? metric.services[0] : undefined);
  const tracesLink = traceService ? `/traces?${new URLSearchParams({ service: traceService })}` : '/traces';
  const unit = metric ? getMetricUnitLabel(metric.unit) : '';
  const subtitle = [metric?.description.replace(/\.$/, ''), unit].filter(Boolean).join(' · ');

  return (
    <div className="flex min-h-full flex-col bg-surface-page">
      <MetricTopBar metric={metric} aggregation={aggregation} service={service} filter={filter} groupBy={groupBy} />
      {metric && aggregation && (
        <div className="flex flex-1 justify-center px-10 pt-3 pb-12">
          <div className="flex w-full max-w-240 min-w-0 flex-col gap-6">
            <div className="flex flex-col gap-1">
              <div className="flex items-center gap-2.5">
                <h1 className="truncate text-title-page font-semibold text-text-primary">{metric.name}</h1>
                <AlertBadge ruleIds={ruleIds} fallback={null} />
              </div>
              <div className="flex flex-wrap items-center gap-2.5">
                {subtitle && <p className="text-body text-text-secondary">{subtitle}</p>}
                <div className="flex flex-wrap gap-1.5">
                  {[getKindLabel(metric), ...metric.services].map((label) => (
                    <span
                      key={label}
                      className="flex h-5.5 items-center rounded-xs bg-surface-fill px-1.75 text-caption font-normal text-text-secondary"
                    >
                      {label}
                    </span>
                  ))}
                </div>
              </div>
            </div>

            <div className="flex flex-col gap-4 lg:flex-row">
              <MetricSpecGrid metric={metric} />
              {metric.kind === 'histogram' ? (
                <MetricTracesCard exemplars={shown?.exemplars} tracesLink={tracesLink} />
              ) : (
                <MetricRulesCard rules={rules} firingRuleIds={firingRuleIds} />
              )}
            </div>

            <AlertBanners ruleIds={ruleIds} />

            <MetricOverTime
              metric={metric}
              result={shown}
              series={series}
              aggregation={shown?.aggregation ?? aggregation}
              range={range}
              groupBy={groupBy}
              filters={filters}
              attributes={attributes}
              limit={limit}
              isFiring={!!limitRule && firingRuleIds.includes(limitRule.id)}
              onChange={setParams}
            />

            <MetricSeriesSection
              metric={metric}
              series={series}
              rangeMs={range.ms}
              service={service}
              serverId={serverId}
              filter={filter}
              groupBy={groupBy}
            />
          </div>
        </div>
      )}
    </div>
  );
}
