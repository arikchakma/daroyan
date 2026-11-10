import { useQuery } from '@tanstack/react-query';

import type { LogRow } from '~/hooks/use-logs-table';
import { logQueries } from '~/queries/logs';

export function useFullLogLine(line: LogRow) {
  const { data: context, isLoading } = useQuery(
    logQueries.context({ server_id: line.server_id, service: line.service, at: line.at })
  );

  const match = context?.find((item) => item.at === line.at && item.message === line.message);
  const full: LogRow = { ...line, raw: match?.raw, resource: match?.resource };

  return { context, isLoading, full };
}
