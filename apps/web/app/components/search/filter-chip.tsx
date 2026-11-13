type FilterChipProps = {
  condition: string;
  value: string;
};

export function FilterChip(props: FilterChipProps) {
  const { condition, value } = props;

  if (!value) {
    return (
      <span className="inline-flex h-5 max-w-full items-center rounded-sm bg-filter-label px-1.5 text-caption whitespace-nowrap text-text-primary">
        {condition}
      </span>
    );
  }

  return (
    <span className="inline-flex h-5 max-w-full gap-px text-caption whitespace-nowrap">
      <span className="flex items-center rounded-l-sm rounded-r-[2px] bg-filter-label pr-1 pl-1.5 text-text-primary">
        {condition}
      </span>
      <span className="flex min-w-0 items-center rounded-l-[2px] rounded-r-sm bg-filter-value pr-1.5 pl-1 text-filter-value-fg">
        <span className="truncate">{value}</span>
      </span>
    </span>
  );
}
