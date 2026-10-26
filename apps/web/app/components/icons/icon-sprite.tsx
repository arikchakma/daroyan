import { getIconSymbolId } from '~/utils/icon';

import { FILLED_ICON_PATHS, ICON_PATHS, LOGO_PATHS } from './icon-paths';

export function IconSprite() {
  return (
    <svg className="hidden" aria-hidden="true">
      {Object.entries(ICON_PATHS).map(([name, paths]) => (
        <symbol
          key={name}
          id={getIconSymbolId(name)}
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          strokeWidth={1.45}
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          {paths.map((d) => (
            <path key={d} d={d} />
          ))}
        </symbol>
      ))}
      {Object.entries(FILLED_ICON_PATHS).map(([name, icon]) => (
        <symbol key={name} id={getIconSymbolId(name)} viewBox="0 0 16 16">
          <path d={icon.shape} fill="currentColor" />
          {icon.strokes.map((d) => (
            <path
              key={d}
              d={d}
              fill="none"
              stroke="var(--icon-glyph, var(--color-surface-default))"
              strokeWidth={1.5}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          ))}
          {icon.dots.map((d) => (
            <path
              key={d}
              d={d}
              fill="var(--icon-glyph, var(--color-surface-default))"
            />
          ))}
        </symbol>
      ))}
      {Object.entries(LOGO_PATHS).map(([name, logo]) => (
        <symbol key={name} id={getIconSymbolId(name)} viewBox={logo.viewBox}>
          {logo.paths.map((path) => (
            <path key={path.d} d={path.d} fill={path.fill} />
          ))}
        </symbol>
      ))}
    </svg>
  );
}
