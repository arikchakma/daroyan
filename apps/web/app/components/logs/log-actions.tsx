import { useNavigate } from 'react-router';

import { ActionPill } from '~/components/ui/action-pill';
import { useFullLogLine } from '~/hooks/use-full-log-line';
import type { LogRow } from '~/hooks/use-logs-table';
import { toast } from '~/lib/toast';
import { getLogAlertLink, getLogRecord, OTLP_SERVER_ID } from '~/utils/logs';

type LogActionsProps = {
  line: LogRow;
};

export function LogActions(props: LogActionsProps) {
  const { line } = props;

  const navigate = useNavigate();
  const { full } = useFullLogLine(line);

  const copyLine = async () => {
    await navigator.clipboard.writeText(JSON.stringify(getLogRecord(full), null, 2));
    toast.success('Copied the line as JSON');
  };

  return (
    <>
      <ActionPill icon="copy" tone="dark" onClick={() => void copyLine()}>
        Copy JSON
      </ActionPill>
      {line.server_id !== OTLP_SERVER_ID && (
        <ActionPill icon="bell" tone="light" onClick={() => navigate(getLogAlertLink(line))}>
          Alert on This
        </ActionPill>
      )}
    </>
  );
}
