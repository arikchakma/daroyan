import { useQuery } from '@tanstack/react-query';

import { RuleForm } from '~/components/alerts/rule-form';
import { alertRuleOptions } from '~/queries/alerts';

import type { Route } from './+types/_app.alerts.$ruleId.edit';

export function meta() {
  return [{ title: 'Edit Rule · Daroyan' }];
}

export default function EditRulePage(props: Route.ComponentProps) {
  const { ruleId } = props.params;

  const { data: rule, error } = useQuery(alertRuleOptions(ruleId));

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Rule Not Found</p>
        <p className="text-body text-text-secondary">It may have been deleted.</p>
      </div>
    );
  }

  return rule ? <RuleForm key={rule.id} rule={rule} /> : null;
}
