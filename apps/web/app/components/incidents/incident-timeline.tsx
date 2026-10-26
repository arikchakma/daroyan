import { Icon } from '~/components/icons/icon';
import { IncidentStatusBadge } from '~/components/incidents/incident-badges';
import { RichText } from '~/components/rich-text/rich-text';
import { cn } from '~/utils/classname';
import { formatClock } from '~/utils/format';
import { getInitial, getStatusMeta, groupByDay } from '~/utils/incidents';
import type { IncidentUpdate } from '~/utils/incidents';
import { toDocument, toPlainText } from '~/utils/rich-text';

const MARKER_CLASS_NAMES: Record<string, string> = {
  critical: 'bg-tone-critical-bg text-tone-critical-fg',
  warning: 'bg-tone-warning-bg text-tone-warning-fg',
  info: 'bg-tone-info-bg text-tone-info-fg',
  success: 'bg-tone-success-bg text-tone-success-fg',
  magic: 'bg-tone-magic-bg text-tone-magic-fg',
  neutral: 'bg-tone-neutral-bg text-tone-neutral-fg',
};

type IncidentTimelineProps = {
  updates: IncidentUpdate[];
  shownOn: string;
  onEdit?: (update: IncidentUpdate) => void;
};

export function IncidentTimeline(props: IncidentTimelineProps) {
  const { updates, shownOn, onEdit } = props;

  const groups = groupByDay(updates);
  const last = updates.at(-1);

  if (updates.length === 0) {
    return (
      <div className="rounded-2xl border border-border-ring bg-surface-default px-5 py-4 text-body text-text-secondary shadow-card">
        Nothing on the timeline yet.
      </div>
    );
  }

  return (
    <ol className="flex flex-col rounded-2xl border border-border-ring bg-surface-default px-5 pt-4 pb-1.5 shadow-card">
      {groups.map((group) => (
        <li key={group.day} className="flex flex-col">
          <p className="pb-2.5 text-caption font-medium text-text-secondary">{group.day}</p>
          <ol className="flex flex-col">
            {group.entries.map((update) => (
              <TimelineEntry
                key={update.id}
                update={update}
                shownOn={shownOn}
                isLast={update.id === last?.id}
                onEdit={onEdit}
              />
            ))}
          </ol>
        </li>
      ))}
    </ol>
  );
}

type TimelineEntryProps = {
  update: IncidentUpdate;
  shownOn: string;
  isLast: boolean;
  onEdit?: (update: IncidentUpdate) => void;
};

function TimelineEntry(props: TimelineEntryProps) {
  const { update, shownOn, isLast, onEdit } = props;

  const isAutomatic = update.kind === 'automatic';
  const isNote = update.kind === 'note';
  const body = toDocument(update.body);
  const author = update.creator?.name ?? (update.creator_id ? 'Former user' : 'Daroyan');

  return (
    <li className="group/entry flex gap-3">
      <span className="w-11 shrink-0 pt-px text-small text-text-secondary tabular-nums">
        {formatClock(Date.parse(update.created_at))}
      </span>
      <span aria-hidden="true" className="flex w-5 shrink-0 flex-col items-center gap-1">
        <Marker update={update} />
        {!isLast && <span className="w-0.5 flex-1 rounded-[1px] bg-border-row" />}
      </span>
      <div className={cn('flex min-w-0 flex-1 flex-col gap-1.5', isAutomatic ? 'pb-3.5' : 'pb-5')}>
        {isAutomatic ? (
          <p className="pt-px text-small text-text-secondary">{describeAutomatic(update)}</p>
        ) : (
          <div className={cn('flex flex-col gap-1.5', isNote && 'rounded-xl bg-highlight-note px-3 py-2.5')}>
            <div className="flex min-w-0 items-center gap-1.5">
              <span className="flex size-5 shrink-0 items-center justify-center rounded-[10px] bg-surface-fill text-small text-text-secondary">
                {getInitial(author)}
              </span>
              <span className="truncate text-body font-medium text-text-primary">{author}</span>
              {isNote ? (
                <span className="flex shrink-0 items-center gap-1 text-small text-text-secondary">
                  <Icon name="lock" className="size-3" />
                  Internal
                </span>
              ) : (
                <span className="shrink-0 text-small text-text-secondary">
                  {update.is_public ? shownOn : 'Internal'}
                </span>
              )}
              {update.status && <IncidentStatusBadge status={update.status} hasIcon={false} />}
              <span className="flex-1" />
              {update.edited_at && <span className="shrink-0 text-small text-text-secondary">Edited</span>}
              {onEdit && (
                <button
                  type="button"
                  aria-label="Edit update"
                  onClick={() => onEdit(update)}
                  className="flex size-6 shrink-0 items-center justify-center rounded-full text-icon-secondary opacity-0 outline-none group-hover/entry:opacity-100 hover:bg-surface-fill hover:text-text-primary focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-text-link/40"
                >
                  <Icon name="pencil" />
                </button>
              )}
            </div>
            <RichText content={body} />
          </div>
        )}
      </div>
    </li>
  );
}

function describeAutomatic(update: IncidentUpdate) {
  const text = toPlainText(toDocument(update.body)).trim();
  const name = update.creator?.name;
  if (!name || text.includes(name)) {
    return text;
  }
  return `${text.replace(/\.$/, '')} by ${name}`;
}

function Marker(props: { update: IncidentUpdate }) {
  const { update } = props;

  if (update.kind === 'automatic') {
    return (
      <span className="flex size-5 shrink-0 items-center justify-center">
        <span className="size-1.75 rounded-full bg-icon-secondary" />
      </span>
    );
  }
  if (update.kind === 'note') {
    return (
      <span className="flex size-5 shrink-0 items-center justify-center rounded-[10px] bg-highlight-note text-tone-attention-fg [&_svg]:size-2.75">
        <Icon name="lock" />
      </span>
    );
  }
  const meta = getStatusMeta(update.status ?? 'investigating');
  return (
    <span
      className={cn(
        'flex size-5 shrink-0 items-center justify-center rounded-[10px] [&_svg]:size-3',
        MARKER_CLASS_NAMES[meta.tone]
      )}
    >
      <Icon name={meta.icon} />
    </span>
  );
}
