import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { listAlertRulesOptions } from '~/queries/alerts';
import { isCheckKind } from '~/utils/alerts';

export function getCheckRules(rules: Schemas['AlertRule'][], checkId?: string) {
  return rules.filter(
    (rule) =>
      isCheckKind(rule.condition.kind) &&
      (rule.scope.kind === 'all' || (!!checkId && rule.scope.kind === 'checks' && rule.scope.check_ids.includes(checkId)))
  );
}

type CheckRulesProps = {
  checkId?: string;
  checkName?: string;
};

export function getNewRulePath(checkId?: string, checkName?: string) {
  if (!checkId) {
    return '/alerts/new';
  }
  const params = new URLSearchParams({ checkId, checkName: checkName ?? '' });
  return `/alerts/new?${params}`;
}

export function CheckRules(props: CheckRulesProps) {
  const { checkId, checkName } = props;

  const { data: rules } = useQuery(listAlertRulesOptions());
  const covering = getCheckRules(rules ?? [], checkId);

  return (
    <>
      {checkId && rules && covering.length === 0 && (
        <p className="px-4 py-3 text-body text-text-secondary">
          No alert rule covers this check yet, so nobody hears when it goes down.
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
              {rule.summary} · {rule.scope.kind === 'all' ? 'All checks' : 'Picked checks'}
            </span>
          </div>
          <Icon name="chevron-right" className="shrink-0 text-text-tertiary" />
        </Link>
      ))}
      {checkId && (
        <Link
          to={getNewRulePath(checkId, checkName)}
        className="flex items-center gap-2.5 bg-surface-subtle px-4 py-2.5 text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 focus-visible:ring-inset"
      >
          <Icon name="plus" />
          New Alert Rule
        </Link>
      )}
    </>
  );
}
