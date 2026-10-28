import type { Schemas } from '@daroyan/client';
import type { ReactNode } from 'react';

import { formatDate } from '~/utils/format';

type BacktestBoxProps = {
  backtest?: Schemas['AlertRuleBacktest'];
  savedCount?: number;
};

export function BacktestBox(props: BacktestBoxProps) {
  const { backtest, savedCount } = props;

  if (!backtest) {
    return <BacktestNote>Finish the condition to see how often it would have fired.</BacktestNote>;
  }
  if (!backtest.is_supported) {
    return <BacktestNote>There is no history to check this rule against yet.</BacktestNote>;
  }

  const count = backtest.hits.length;
  const difference = savedCount === undefined ? 0 : count - savedCount;
  const compared = difference === 0 ? '' : `, ${Math.abs(difference)} ${difference < 0 ? 'fewer' : 'more'}`;

  return (
    <div className="flex flex-col gap-2 rounded-2xl bg-surface-subtle p-4 text-body">
      <span className="font-medium">
        {count === 0 ? 'Would not have fired' : `Would have fired ${count === 1 ? 'once' : `${count} times`}`}
        {compared}
      </span>
      {backtest.hits.slice(0, 3).map((hit) => (
        <div key={`${hit.server_id}-${hit.at}`} className="flex items-center gap-3">
          <span className="min-w-0 flex-1 truncate text-text-secondary">
            {hit.server_name} ·{' '}
            {hit.subject === 'CPU' || hit.subject === 'Memory' || hit.subject === 'Load' ? hit.value : hit.subject}
          </span>
          <span className="shrink-0 text-text-tertiary">{formatDate(hit.at)}</span>
        </div>
      ))}
    </div>
  );
}

type BacktestNoteProps = {
  children: ReactNode;
};

function BacktestNote(props: BacktestNoteProps) {
  const { children } = props;

  return <div className="flex flex-col gap-2 rounded-2xl bg-surface-subtle p-4 text-body text-text-secondary">{children}</div>;
}
