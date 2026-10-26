import { createCn } from 'cn/config';

export const cn = createCn({
  extend: {
    theme: {
      text: [
        'display',
        'hero',
        'title-page',
        'greeting',
        'title-auth',
        'title-section',
        'title-card',
        'body',
        'small',
        'caption',
        'stat',
        'axis',
        'tiny',
        'code',
        'code-small',
      ],
      radius: ['tile'],
      shadow: [
        'card',
        'button-primary',
        'button-secondary',
        'popover',
        'segment',
        'toolbar',
        'bubble',
        'float',
        'panel-float',
        'filter-bar',
        'row-gap',
      ],
    },
  },
});
