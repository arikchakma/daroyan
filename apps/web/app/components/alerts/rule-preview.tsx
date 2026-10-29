import type { Schemas } from '@daroyan/client';
import { keepPreviousData, useQuery } from '@tanstack/react-query';

import { BacktestBox } from '~/components/alerts/backtest-box';
import { backtestAlertRuleOptions } from '~/queries/alerts';
import type { Scope } from '~/utils/alerts';

type RulePreviewProps = {
  rule?: Schemas['AlertRule'];
  condition?: Schemas['Condition'];
  scope?: Scope;
  forSecs: number;
};

export function RulePreview(props: RulePreviewProps) {
  const { rule, condition, scope, forSecs } = props;

  const { data: backtest } = useQuery({
    ...backtestAlertRuleOptions({ condition: condition ?? { kind: 'offline' }, for_secs: forSecs, scope: scope ?? { kind: 'all' } }),
    enabled: !!condition && !!scope,
    placeholderData: keepPreviousData,
  });
  const { data: savedBacktest } = useQuery({
    ...backtestAlertRuleOptions({
      condition: rule?.condition ?? { kind: 'offline' },
      for_secs: rule?.for_secs,
      scope: rule?.scope ?? { kind: 'all' },
    }),
    enabled: !!rule,
  });

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3 text-body">
        <h2 className="font-medium">{rule ? 'With These Changes' : 'Preview'}</h2>
        <span className="text-text-tertiary">Last {backtest?.days ?? 7} days</span>
      </div>
      <BacktestBox
        backtest={condition && scope ? backtest : undefined}
        savedCount={rule && savedBacktest?.is_supported ? savedBacktest.hits.length : undefined}
      />
    </div>
  );
}
