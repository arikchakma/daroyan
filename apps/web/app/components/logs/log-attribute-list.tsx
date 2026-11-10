import { LogDetailRow } from '~/components/logs/log-detail-row';

type LogAttributeListProps = {
  title: string;
  entries: [string, unknown][];
};

export function LogAttributeList(props: LogAttributeListProps) {
  const { title, entries } = props;

  return (
    <div className="flex flex-col gap-2">
      <h2 className="text-body font-medium">{title}</h2>
      <dl className="flex flex-col gap-2 text-body">
        {entries.map(([key, value]) => (
          <LogDetailRow key={key} label={key} mono>
            {typeof value === 'string' ? value : JSON.stringify(value)}
          </LogDetailRow>
        ))}
      </dl>
    </div>
  );
}
