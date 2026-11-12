import { LogDetailRow } from '~/components/logs/log-detail-row';
import { LogFilterActions } from '~/components/logs/log-filter-actions';
import type { QueryTerm } from '~/utils/log-search';

type LogAttributeListProps = {
  title: string;
  entries: [string, unknown][];
  onFilter?: (term: QueryTerm) => void;
};

export function LogAttributeList(props: LogAttributeListProps) {
  const { title, entries, onFilter } = props;

  return (
    <div className="flex flex-col gap-2">
      <h2 className="text-body font-medium">{title}</h2>
      <dl className="flex flex-col gap-2 text-body">
        {entries.map(([key, value]) => {
          const text = typeof value === 'string' ? value : JSON.stringify(value);
          const canFilter = onFilter && (typeof value !== 'object' || value === null);

          return (
            <LogDetailRow
              key={key}
              label={key}
              mono
              actions={canFilter && <LogFilterActions name={key} value={text} onFilter={onFilter} />}
            >
              {text}
            </LogDetailRow>
          );
        })}
      </dl>
    </div>
  );
}
