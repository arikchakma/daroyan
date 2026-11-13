import type { QueryTerm } from '~/utils/log-search';

type LogFilterActionsProps = {
  name: string;
  value: string;
  onFilter: (term: QueryTerm) => void;
};

export function LogFilterActions(props: LogFilterActionsProps) {
  const { name, value, onFilter } = props;

  return (
    <div className="flex shrink-0 gap-1 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
      <button
        type="button"
        onClick={() => onFilter({ key: name, kind: 'text', values: [value], negate: false })}
        className="h-5 rounded-xs px-1.5 text-caption font-medium text-text-secondary outline-none hover:bg-surface-fill hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40"
      >
        Show only
      </button>
      <button
        type="button"
        onClick={() => onFilter({ key: name, kind: 'text', values: [value], negate: true })}
        className="h-5 rounded-xs px-1.5 text-caption font-medium text-text-secondary outline-none hover:bg-surface-fill hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40"
      >
        Exclude
      </button>
    </div>
  );
}
