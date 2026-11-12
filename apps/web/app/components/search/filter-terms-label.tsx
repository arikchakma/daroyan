import { getTermCondition, getTermValue } from '~/utils/log-search';
import type { QueryTerm } from '~/utils/log-search';

import { FilterChip } from './filter-chip';

type FilterTermsLabelProps = {
  terms: QueryTerm[];
};

export function FilterTermsLabel(props: FilterTermsLabelProps) {
  const { terms } = props;

  if (terms.length === 1) {
    const term = terms[0]!;
    return (
      <>
        {getTermCondition(term)} <span className="font-medium">{getTermValue(term)}</span>
      </>
    );
  }

  return (
    <span className="flex items-center gap-1">
      {terms.map((term, index) => (
        <FilterChip key={index} condition={getTermCondition(term)} value={getTermValue(term)} />
      ))}
    </span>
  );
}
