import { Mark } from '@tiptap/core';

import { FILTER_SEARCH_MARK } from '~/utils/filter';

export const FilterSearchMark = Mark.create({
  name: FILTER_SEARCH_MARK,
  inclusive: true,

  parseHTML() {
    return [{ tag: 'span[data-filter-search]' }];
  },

  renderHTML() {
    return ['span', { 'data-filter-search': '', class: 'mr-1' }, 0];
  },
});
