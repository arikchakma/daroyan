import { useSearchParams } from 'react-router';

import { RuleForm } from '~/components/alerts/rule-form';
import type { RuleDraft } from '~/utils/rule-form';

export function meta() {
  return [{ title: 'New Rule · Daroyan' }];
}

function readLogDraft(params: URLSearchParams): RuleDraft | undefined {
  const service = params.get('logService');
  if (!service) {
    return undefined;
  }
  const level = params.get('logLevel') ?? 'error';
  const text = params.get('logText') || null;
  return {
    name: text ? `${service}: ${text.slice(0, 40)}` : `${service} ${level === 'warn' ? 'warnings' : 'errors'}`,
    condition: { kind: 'logs', level, count: 1, within_secs: 300, service, text },
  };
}

function readCheckDraft(params: URLSearchParams): RuleDraft | undefined {
  const checkId = params.get('checkId');
  if (!checkId) {
    return undefined;
  }
  return {
    name: `${params.get('checkName') || 'Check'} Down`,
    condition: { kind: 'uptime_down' },
    scope: { kind: 'checks', check_ids: [checkId] },
    forSecs: 0,
  };
}

function readCronDraft(params: URLSearchParams): RuleDraft | undefined {
  const cronId = params.get('cronId');
  if (!cronId) {
    return undefined;
  }
  return {
    name: `${params.get('cronName') || 'Cron'} Failed`,
    condition: { kind: 'cron_failed' },
    scope: { kind: 'crons', cron_ids: [cronId] },
    forSecs: 0,
  };
}

export default function NewRulePage() {
  const [searchParams] = useSearchParams();

  return <RuleForm draft={readCheckDraft(searchParams) ?? readCronDraft(searchParams) ?? readLogDraft(searchParams)} />;
}
