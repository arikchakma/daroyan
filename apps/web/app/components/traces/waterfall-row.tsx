import { Icon } from '~/components/icons/icon';
import { ServiceDot } from '~/hooks/use-traces-table';
import { cn } from '~/utils/classname';
import { ERROR_COLOR, TICKS, formatMicros } from '~/utils/traces';
import type { SpanNode } from '~/utils/traces';

const INDENT = 16;
const GUIDE_X = 6;

type WaterfallRowProps = {
  node: SpanNode;
  isRoot: boolean;
  isCollapsed: boolean;
  onToggle: () => void;
  color: string;
  offset: number;
  width: number;
  isSelected: boolean;
  onSelect: () => void;
};

export function WaterfallRow(props: WaterfallRowProps) {
  const { node, isRoot, isCollapsed, onToggle, color, offset, width, isSelected, onSelect } = props;

  const isError = node.status === 'error';
  const isLabelBefore = offset + width > 0.7;
  const ownX = (node.depth - 1) * INDENT + GUIDE_X;

  return (
    <li className="relative">
      <button
        type="button"
        aria-pressed={isSelected}
        onClick={onSelect}
        className="flex h-8.5 w-full items-stretch border-t border-border-row px-4 text-left outline-none hover:bg-surface-subtle focus-visible:bg-surface-subtle aria-pressed:bg-surface-subtle"
      >
        <span className="relative flex w-75 shrink-0 items-center overflow-hidden">
          {node.guides.map(
            (isShown, index) =>
              isShown && (
                <span
                  key={index}
                  aria-hidden="true"
                  className="absolute -top-px bottom-0 w-px bg-border-strong"
                  style={{ left: index * INDENT + GUIDE_X }}
                />
              )
          )}
          {node.depth > 0 && (
            <>
              <span
                aria-hidden="true"
                className={cn('absolute -top-px w-px bg-border-strong', node.isLast ? 'h-[calc(50%+1px)]' : 'bottom-0')}
                style={{ left: ownX }}
              />
              <span
                aria-hidden="true"
                className="absolute top-1/2 h-px bg-border-strong"
                style={{ left: ownX, width: INDENT + (node.hasChildren ? -GUIDE_X - 2 : GUIDE_X + 3) }}
              />
            </>
          )}
          {node.hasChildren && !isCollapsed && (
            <span
              aria-hidden="true"
              className="absolute top-[calc(50%+7px)] bottom-0 w-px bg-border-strong"
              style={{ left: node.depth * INDENT + GUIDE_X }}
            />
          )}
          <span
            className="flex min-w-0 items-center gap-2"
            style={{ paddingLeft: node.depth * INDENT + GUIDE_X * 2 + 6 }}
          >
            <ServiceDot service={node.service} color={color} />
            <span
              className={cn(
                'truncate font-mono',
                isRoot ? 'text-code font-medium' : 'text-code-small',
                isError ? 'text-tone-critical-fg' : 'text-text-primary'
              )}
            >
              {node.name}
            </span>
          </span>
        </span>
        <span className="relative flex-1">
          {Array.from({ length: TICKS }, (_, index) => (
            <span
              key={index}
              aria-hidden="true"
              className="absolute -top-px bottom-0 w-px bg-border-row"
              style={{ left: `${(index / (TICKS - 1)) * 100}%` }}
            />
          ))}
          <span
            className="absolute top-1/2 h-2 -translate-y-1/2 rounded-[2px]"
            style={{
              left: `${offset * 100}%`,
              width: `max(${width * 100}%, 2px)`,
              backgroundColor: isError ? ERROR_COLOR : color,
            }}
          />
          <span
            className="absolute top-1/2 -translate-y-1/2 font-mono text-code-small whitespace-nowrap text-text-tertiary"
            style={
              isLabelBefore
                ? { right: `calc(${(1 - offset) * 100}% + 6px)` }
                : { left: `calc(${(offset + width) * 100}% + 6px)` }
            }
          >
            {formatMicros(node.duration_us)}
          </span>
        </span>
      </button>
      {node.hasChildren && (
        <button
          type="button"
          aria-label={isCollapsed ? `Expand ${node.name}` : `Collapse ${node.name}`}
          aria-expanded={!isCollapsed}
          onClick={onToggle}
          className="absolute top-1/2 flex size-4 -translate-y-1/2 items-center justify-center rounded-xs text-text-tertiary outline-none hover:bg-surface-fill hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40"
          style={{ left: 16 + node.depth * INDENT + GUIDE_X - 8 }}
        >
          <Icon name={isCollapsed ? 'chevron-right' : 'chevron-down'} className="size-3" />
        </button>
      )}
    </li>
  );
}
