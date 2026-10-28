import { useQuery } from '@tanstack/react-query';

import { RuleBody } from '~/components/alerts/rule-body';
import { PageBar } from '~/components/ui/page-bar';
import { alertRuleOptions } from '~/queries/alerts';

import type { Route } from './+types/_app.alerts.$ruleId._index';

export function meta() {
  return [{ title: 'Alert Rule · Daroyan' }];
}

export default function AlertRulePage(props: Route.ComponentProps) {
  const { ruleId } = props.params;

  const { data: rule, error } = useQuery(alertRuleOptions(ruleId));

  if (error) {
    return (
      <div className="flex h-full flex-col">
        <PageBar backTo="/alerts" icon="bell" title="Alerts" />
        <div className="flex flex-1 flex-col items-center justify-center gap-1 pb-20">
          <p className="text-title-section font-medium text-text-primary">Rule Not Found</p>
          <p className="text-body text-text-secondary">It may have been deleted.</p>
        </div>
      </div>
    );
  }

  return rule ? <RuleBody rule={rule} /> : <PageBar backTo="/alerts" icon="bell" title="Alerts" />;
}
