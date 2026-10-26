import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { listAlertRulesOptions } from '~/queries/alerts';
import { isCronKind } from '~/utils/alerts';

export function getCronRules(rules: Schemas['AlertRule'][], cronId?: string) {
  return rules.filter(
    (rule) =>
      isCronKind(rule.condition.kind) &&
      (rule.scope.kind === 'all' || (!!cronId && rule.scope.kind === 'crons' && rule.scope.cron_ids.includes(cronId)))
  );
}

export function getNewCronRulePath(cronId: string, cronName: string) {
  return `/alerts/new?${new URLSearchParams({ cronId, cronName })}`;
}

type CronRulesProps = {
  cronId: string;
  cronName: string;
};

export function CronRules(props: CronRulesProps) {
  const { cronId, cronName } = props;

  const { data: rules } = useQuery(listAlertRulesOptions());
  const covering = getCronRules(rules ?? [], cronId);

  return (
    <>
      {rules && covering.length === 0 && (
        <p className="px-4 py-3 text-body text-text-secondary">
          No alert rule covers this cron yet, so nobody hears when it misses a run.
        </p>
      )}
      {covering.map((rule) => (
        <Link
          key={rule.id}
          to={`/alerts/${rule.id}`}
          className="flex items-center gap-3 px-4 py-3 outline-none hover:bg-surface-subtle focus-visible:bg-surface-subtle"
        >
          <Icon name={rule.enabled ? 'bell' : 'bell-off'} className="shrink-0" />
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="truncate text-body font-medium">{rule.name}</span>
            <span className="truncate text-body text-text-secondary">
              {rule.summary} · {rule.scope.kind === 'all' ? 'All crons' : 'Picked crons'}
            </span>
          </div>
          <Icon name="chevron-right" className="shrink-0 text-text-tertiary" />
        </Link>
      ))}
      <Link
        to={getNewCronRulePath(cronId, cronName)}
        className="flex items-center gap-2.5 bg-surface-subtle px-4 py-2.5 text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 focus-visible:ring-inset"
      >
        <Icon name="plus" />
        New Alert Rule
      </Link>
    </>
  );
}
