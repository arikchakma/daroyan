import { LogAttributeList } from '~/components/logs/log-attribute-list';
import { LogDetailRow } from '~/components/logs/log-detail-row';
import { describeRequest, formatDuration, HTTP_KEYS } from '~/hooks/use-logs-table';
import type { LogRow } from '~/hooks/use-logs-table';
import { formatLogStamp } from '~/utils/format';

type LogDetailsProps = {
  line: LogRow;
};

export function LogDetails(props: LogDetailsProps) {
  const { line } = props;

  const values = (line.attributes ?? {}) as Record<string, unknown>;
  const request = describeRequest(line);
  const duration = formatDuration(values['duration_ms']);
  const attributes = Object.entries(values).filter(([key]) => !HTTP_KEYS.includes(key) && key !== 'stack.lines');
  const resource = Object.entries((line.resource ?? {}) as Record<string, unknown>);

  return (
    <aside aria-label="Line Details" className="flex w-90 shrink-0 flex-col gap-5 p-6 max-lg:w-full max-lg:px-4">
      <div className="flex flex-col gap-2">
        <h2 className="text-body font-medium">Details</h2>
        <dl className="flex flex-col gap-2 text-body">
          <LogDetailRow label="Time" mono>
            {formatLogStamp(line.at)}
          </LogDetailRow>
          {request && (
            <LogDetailRow label="Request" mono>
              {request}
            </LogDetailRow>
          )}
          {duration && <LogDetailRow label="Duration">{duration}</LogDetailRow>}
          <LogDetailRow label="Server">{line.serverName}</LogDetailRow>
          <LogDetailRow label="Source">{line.service}</LogDetailRow>
          <LogDetailRow label="Stream">{line.stream}</LogDetailRow>
          {line.trace_id && (
            <LogDetailRow label="Trace" mono>
              {line.trace_id}
            </LogDetailRow>
          )}
        </dl>
      </div>
      {attributes.length > 0 && <LogAttributeList title="Attributes" entries={attributes} />}
      {resource.length > 0 && <LogAttributeList title="Resource" entries={resource} />}
    </aside>
  );
}
