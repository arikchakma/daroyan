import { useQuery } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { useContext } from 'react';
import { useNavigate } from 'react-router';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { PanelContext } from '~/components/navigation/panel-context';
import { Sheet } from '~/components/ui/sheet';
import { Skeleton } from '~/components/ui/skeleton';
import { describeRequest, formatAttributes, formatDuration, HTTP_KEYS, LevelBadge } from '~/hooks/use-logs-table';
import type { LogRow } from '~/hooks/use-logs-table';
import { toast } from '~/lib/toast';
import { logContextOptions } from '~/queries/logs';
import { cn } from '~/utils/classname';
import { formatLogClock, formatLogStamp } from '~/utils/format';

type LogSheetProps = {
  line?: LogRow;
  canGoPrevious: boolean;
  canGoNext: boolean;
  onNavigate: (step: -1 | 1) => void;
  onClose: () => void;
};

function toRecord(line: LogRow) {
  return {
    time: new Date(line.at).toISOString(),
    server: line.serverName,
    source: line.service,
    stream: line.stream,
    level: line.level,
    message: line.message,
    attributes: line.attributes ?? {},
    raw: line.raw ?? line.message,
    ...(line.trace_id && { traceId: line.trace_id }),
  };
}

function buildAlertLink(line: LogRow) {
  const params = new URLSearchParams({ logService: line.service, logLevel: line.level === 'warn' ? 'warn' : 'error' });
  const text = line.message.split('\n')[0]?.trim() ?? '';
  if (!describeRequest(line) && text) {
    params.set('logText', text.slice(0, 120));
  }
  return `/alerts/new?${params}`;
}

export function LogSheet(props: LogSheetProps) {
  const { line, canGoPrevious, canGoNext, onNavigate, onClose } = props;

  const panel = useContext(PanelContext);
  const navigate = useNavigate();

  return (
    <Sheet.Root side="bottom" open={!!line} onOpenChange={(open) => !open && onClose()}>
      <Sheet.Portal container={panel}>
        <Sheet.Backdrop className="bg-black opacity-[calc(0.4*(1-var(--drawer-swipe-progress)))]" />
        <Sheet.Viewport className={cn('pt-4 pb-1', panel ? 'absolute pr-1' : 'fixed px-1')}>
          <Sheet.Popup className="h-auto flex-1 gap-0 rounded-2xl border-0 bg-surface-page text-text-primary shadow-popover ring-1 ring-border-elevated transition-[translate,opacity] ring-inset data-ending-style:translate-y-4 data-ending-style:opacity-0 data-starting-style:translate-y-4 data-starting-style:opacity-0">
            <div className="bar-stuck grid h-14 shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-3 px-4">
              <div className="flex min-w-0 items-center gap-2">
                <Icon name="terminal" className="shrink-0" />
                <Sheet.Title className="truncate font-mono text-code font-normal">
                  {line ? `${line.service} · ${formatLogClock(line.at)}` : 'Log Line'}
                </Sheet.Title>
              </div>
              <div className="flex items-center gap-1 rounded-full bg-surface-code p-1 empty:hidden">
                {line && (
                  <>
                    <ActionPill
                      icon="copy"
                      tone="dark"
                      onClick={() =>
                        void navigator.clipboard
                          .writeText(JSON.stringify(toRecord(line), null, 2))
                          .then(() => toast.success('Copied the line as JSON'))
                      }
                    >
                      Copy JSON
                    </ActionPill>
                    <ActionPill icon="bell" tone="light" onClick={() => navigate(buildAlertLink(line))}>
                      Alert on This
                    </ActionPill>
                  </>
                )}
              </div>
              <div className="flex items-center justify-end gap-2">
                <div className="flex">
                  <IconButton label="Next Line" icon="chevron-down" disabled={!canGoNext} onClick={() => onNavigate(1)} />
                  <IconButton
                    label="Previous Line"
                    icon="chevron-up"
                    disabled={!canGoPrevious}
                    onClick={() => onNavigate(-1)}
                  />
                </div>
                <Sheet.Close
                  aria-label="Close"
                  className="flex size-8 items-center justify-center rounded-full border border-border-ring bg-surface-default shadow-button-secondary outline-none hover:bg-surface-subtle focus-visible:ring-2 focus-visible:ring-text-link/40"
                >
                  <Icon name="x" />
                </Sheet.Close>
              </div>
            </div>
            {line && <LogBody key={line.id} line={line} />}
          </Sheet.Popup>
        </Sheet.Viewport>
      </Sheet.Portal>
    </Sheet.Root>
  );
}

type LogBodyProps = {
  line: LogRow;
};

function LogBody(props: LogBodyProps) {
  const { line } = props;

  const { data: context, isLoading } = useQuery(
    logContextOptions({ server_id: line.server_id, service: line.service, at: line.at })
  );
  const values = (line.attributes ?? {}) as Record<string, unknown>;
  const request = describeRequest(line);
  const duration = formatDuration(values['duration_ms']);
  const attributes = Object.entries(values).filter(([key]) => !HTTP_KEYS.includes(key) && key !== 'stack.lines');

  return (
    <div className="flex pb-16 max-lg:flex-col">
      <div className="flex min-w-0 flex-1 flex-col gap-6 px-10 pt-6 max-lg:px-4">
        <div className="flex flex-wrap items-center gap-3">
          <LevelBadge level={line.level} />
          <p className="text-body text-text-secondary">
            {line.service} wrote this to {line.stream} on {line.serverName}.
          </p>
        </div>

        <section className="flex flex-col gap-2.5">
          <h2 className="text-title-section font-medium">Message</h2>
          <pre className="max-h-80 overflow-auto rounded-xl bg-surface-code px-4 py-3 font-mono text-code-small wrap-anywhere whitespace-pre-wrap text-white">
            {request ? `${request} · ${line.message}` : line.message}
          </pre>
        </section>

        {line.raw && (
          <section className="flex flex-col gap-2.5">
            <div className="flex flex-col gap-0.5">
              <h2 className="text-title-section font-medium">Raw Line</h2>
              <p className="text-body text-text-secondary">Exactly as {line.service} wrote it, before parsing.</p>
            </div>
            <pre className="max-h-60 overflow-auto rounded-xl border border-border-ring bg-surface-default px-4 py-3 font-mono text-code-small wrap-anywhere whitespace-pre-wrap shadow-card">
              {line.raw}
            </pre>
          </section>
        )}

        <section className="flex flex-col gap-2.5">
          <div className="flex flex-col gap-0.5">
            <h2 className="text-title-section font-medium">Before and After</h2>
            <p className="text-body text-text-secondary">What {line.service} wrote just before and after this line.</p>
          </div>
          <ol className="flex flex-col overflow-hidden rounded-xl border border-border-ring bg-surface-default py-1 shadow-card">
            {isLoading &&
              Array.from({ length: 8 }, (_, index) => (
                <li key={index} className="px-4 py-1.5">
                  <Skeleton className={index % 2 ? 'w-3/4' : 'w-1/2'} />
                </li>
              ))}
            {context?.map((item, index) => {
              const isCurrent = item.at === line.at && item.message === line.message;
              return (
                <li
                  key={`${item.at}-${index}`}
                  className={cn(
                    'relative flex gap-4 px-4 py-1 font-mono text-code-small',
                    isCurrent && 'bg-surface-fill',
                    item.level === 'error' && 'text-tone-critical-fg'
                  )}
                >
                  <span
                    aria-hidden="true"
                    className={cn(
                      'absolute top-1 bottom-1 left-0 w-0.5 rounded-full',
                      item.level === 'error' && 'bg-brand',
                      item.level === 'warn' && 'bg-toast-warning-icon'
                    )}
                  />
                  <span className="w-24 shrink-0 text-text-tertiary tabular-nums">{formatLogClock(item.at)}</span>
                  <span className="min-w-0 wrap-anywhere whitespace-pre-wrap">
                    {[describeRequest(item), item.message || formatAttributes(item.attributes)]
                      .filter(Boolean)
                      .join(' · ')}
                  </span>
                </li>
              );
            })}
          </ol>
        </section>
      </div>

      <aside aria-label="Line Details" className="flex w-90 shrink-0 flex-col gap-5 p-6 max-lg:w-full max-lg:px-4">
        <div className="flex flex-col gap-2">
          <h2 className="text-body font-medium">Details</h2>
          <dl className="flex flex-col gap-2 text-body">
            <RailRow label="Time" mono>
              {formatLogStamp(line.at)}
            </RailRow>
            {request && (
              <RailRow label="Request" mono>
                {request}
              </RailRow>
            )}
            {duration && <RailRow label="Duration">{duration}</RailRow>}
            <RailRow label="Server">{line.serverName}</RailRow>
            <RailRow label="Source">{line.service}</RailRow>
            <RailRow label="Stream">{line.stream}</RailRow>
            {line.trace_id && (
              <RailRow label="Trace" mono>
                {line.trace_id}
              </RailRow>
            )}
          </dl>
        </div>
        {attributes.length > 0 && (
          <div className="flex flex-col gap-2">
            <h2 className="text-body font-medium">Attributes</h2>
            <dl className="flex flex-col gap-2 text-body">
              {attributes.map(([key, value]) => (
                <RailRow key={key} label={key} mono>
                  {typeof value === 'string' ? value : JSON.stringify(value)}
                </RailRow>
              ))}
            </dl>
          </div>
        )}
      </aside>
    </div>
  );
}

type RailRowProps = {
  label: string;
  mono?: boolean;
  children: ReactNode;
};

function RailRow(props: RailRowProps) {
  const { label, mono, children } = props;

  return (
    <div className="flex gap-3">
      <dt className="w-24 shrink-0 truncate text-text-secondary">{label}</dt>
      <dd className={cn('min-w-0 flex-1 wrap-anywhere', mono && 'font-mono text-code')}>{children}</dd>
    </div>
  );
}

type ActionPillProps = {
  icon: IconName;
  tone: 'dark' | 'light';
  onClick: () => void;
  children: ReactNode;
};

function ActionPill(props: ActionPillProps) {
  const { icon, tone, onClick, children } = props;

  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'flex h-7 items-center gap-1.5 rounded-full pr-3 pl-2 text-body font-medium outline-none focus-visible:ring-2 focus-visible:ring-text-link/60 [&_svg]:size-3.5',
        tone === 'dark' && 'bg-sidebar-item-active text-text-on-dark',
        tone === 'light' && 'bg-surface-default text-text-primary'
      )}
    >
      <Icon name={icon} />
      {children}
    </button>
  );
}

type IconButtonProps = {
  label: string;
  icon: IconName;
  disabled: boolean;
  onClick: () => void;
};

function IconButton(props: IconButtonProps) {
  const { label, icon, disabled, onClick } = props;

  return (
    <button
      type="button"
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
      className="flex size-8 items-center justify-center rounded-full border border-border-ring bg-surface-default shadow-button-secondary outline-none hover:bg-surface-subtle focus-visible:ring-2 focus-visible:ring-text-link/40 disabled:opacity-40"
    >
      <Icon name={icon} />
    </button>
  );
}
