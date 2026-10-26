import type { Schemas } from '@daroyan/client';
import type { ReactNode } from 'react';
import { useContext } from 'react';
import { useNavigate } from 'react-router';

import { CronStatusBadge } from '~/components/crons/cron-status-badge';
import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { PanelContext } from '~/components/navigation/panel-context';
import { Button } from '~/components/ui/button';
import { Sheet } from '~/components/ui/sheet';
import { toast } from '~/lib/toast';
import { cn } from '~/utils/classname';
import { formatLateness, formatOutputSize, formatRunTime } from '~/utils/crons';

type RunSheetProps = {
  cron: Schemas['Cron'];
  run?: Schemas['CronRun'];
  canGoPrevious: boolean;
  canGoNext: boolean;
  onNavigate: (step: -1 | 1) => void;
  onClose: () => void;
};

const SECOND_FORMAT = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', second: '2-digit' });

function formatSecond(value: number | null | undefined) {
  return value === null || value === undefined ? '—' : SECOND_FORMAT.format(new Date(value));
}

function describeRun(cron: Schemas['Cron'], run: Schemas['CronRun']) {
  const where = cron.server_name ? ` on ${cron.server_name}` : '';
  const took = run.duration_ms === null || run.duration_ms === undefined ? '' : ` after ${formatRunTime(run.duration_ms)}`;
  switch (run.status) {
    case 'missed':
      return `${cron.name} did not ping within the grace period of its ${formatSecond(run.scheduled_at)} run.`;
    case 'failed':
      return run.exit_code === null || run.exit_code === undefined
        ? `${cron.name} failed${where}${took}.`
        : `${cron.name} failed${where} with exit code ${run.exit_code}${took}.`;
    default: {
      const lateness = formatLateness(run);
      const exit = run.exit_code === null || run.exit_code === undefined ? 'finished' : `exited ${run.exit_code}`;
      return `${cron.name} ran${where} and ${exit}${took}.${lateness === 'On time' || lateness === '—' ? '' : ` It started ${lateness}.`}`;
    }
  }
}

function copyOutput(output: string) {
  void navigator.clipboard.writeText(output).then(() => toast.success('Copied the output'));
}

export function RunSheet(props: RunSheetProps) {
  const { cron, run, canGoPrevious, canGoNext, onNavigate, onClose } = props;

  const panel = useContext(PanelContext);
  const navigate = useNavigate();

  return (
    <Sheet.Root side="bottom" open={!!run} onOpenChange={(open) => !open && onClose()}>
      <Sheet.Portal container={panel}>
        <Sheet.Backdrop className="bg-black opacity-[calc(0.3*(1-var(--drawer-swipe-progress)))]" />
        <Sheet.Viewport className={cn('pt-4 pb-1', panel ? 'absolute pr-1' : 'fixed px-1')}>
          <Sheet.Popup className="h-auto flex-1 gap-0 rounded-2xl border-0 bg-surface-page text-text-primary shadow-popover ring-1 ring-border-elevated transition-[translate,opacity] ring-inset data-ending-style:translate-y-4 data-ending-style:opacity-0 data-starting-style:translate-y-4 data-starting-style:opacity-0">
            <div className="bar-stuck grid h-14 shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-3 px-4">
              <div className="flex min-w-0 items-center gap-2">
                <Icon name="clock" className="shrink-0" />
                <Sheet.Title className="truncate font-mono text-code font-normal">
                  {run ? `${cron.name} · ${formatSecond(run.started_at ?? run.finished_at)}` : 'Run'}
                </Sheet.Title>
              </div>
              <div className="flex items-center gap-1 rounded-full bg-surface-code p-1 empty:hidden">
                {run && (
                  <>
                    <ActionPill
                      icon="copy"
                      tone="dark"
                      disabled={!run.output}
                      onClick={() => run.output && copyOutput(run.output)}
                    >
                      Copy Output
                    </ActionPill>
                    <ActionPill
                      icon="terminal"
                      tone="light"
                      onClick={() => navigate(cron.server_id ? `/logs?server_id=${cron.server_id}` : '/logs')}
                    >
                      View Logs
                    </ActionPill>
                  </>
                )}
              </div>
              <div className="flex items-center justify-end gap-2">
                <div className="flex">
                  <IconButton label="Older Run" icon="chevron-down" disabled={!canGoNext} onClick={() => onNavigate(1)} />
                  <IconButton
                    label="Newer Run"
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
            {run && <RunBody cron={cron} run={run} />}
          </Sheet.Popup>
        </Sheet.Viewport>
      </Sheet.Portal>
    </Sheet.Root>
  );
}

type RunBodyProps = {
  cron: Schemas['Cron'];
  run: Schemas['CronRun'];
};

function RunBody(props: RunBodyProps) {
  const { cron, run } = props;

  const lines = run.output?.split('\n') ?? [];

  return (
    <div className="flex pb-16 max-lg:flex-col">
      <div className="flex min-w-0 flex-1 flex-col gap-6 px-10 pt-6 max-lg:px-4">
        <div className="flex flex-wrap items-center gap-3">
          <CronStatusBadge status={run.status} />
          <p className="text-body text-text-secondary">{describeRun(cron, run)}</p>
        </div>

        <section className="flex flex-col gap-2.5">
          <div className="flex items-end gap-3">
            <div className="flex min-w-0 flex-1 flex-col gap-0.5">
              <h2 className="text-title-section font-medium">Output</h2>
              <p className="text-body text-text-secondary">What the job sent with its ping, up to 10 kB.</p>
            </div>
            {run.output && (
              <>
                <span className="text-body text-text-secondary">{formatOutputSize(run.output)}</span>
                <Button variant="secondary" className="w-fit" onClick={() => run.output && copyOutput(run.output)}>
                  Copy
                </Button>
              </>
            )}
          </div>
          {run.output ? (
            <pre className="h-80 overflow-auto rounded-xl bg-surface-code px-4 py-3 font-mono text-code-small wrap-anywhere whitespace-pre-wrap text-white">
              {lines.map((line, index) => (
                <span
                  key={index}
                  className={cn('block', run.status === 'failed' && index === lines.length - 1 && 'text-brand')}
                >
                  {line || ' '}
                </span>
              ))}
            </pre>
          ) : (
            <p className="flex h-30 items-center justify-center rounded-xl border border-dashed border-border-ring px-4 text-center text-body text-text-secondary">
              No output. Send the job's output in the ping body to see it here.
            </p>
          )}
        </section>
      </div>

      <aside aria-label="Run Details" className="flex w-90 shrink-0 flex-col gap-2 p-6 max-lg:w-full max-lg:px-4">
        <h2 className="text-body font-medium">Details</h2>
        <dl className="flex flex-col gap-2 text-body">
          <RailRow label="Scheduled">{formatSecond(run.scheduled_at)}</RailRow>
          <RailRow label="Started">{formatSecond(run.started_at)}</RailRow>
          <RailRow label="Finished">{run.status === 'missed' ? '—' : formatSecond(run.finished_at)}</RailRow>
          <RailRow label="Duration">{formatRunTime(run.duration_ms)}</RailRow>
          <RailRow label="Started Late">{run.status === 'missed' ? '—' : formatLateness(run)}</RailRow>
          <RailRow label="Exit Code">{run.exit_code ?? '—'}</RailRow>
          <RailRow label="Server">{cron.server_name ?? '—'}</RailRow>
        </dl>
      </aside>
    </div>
  );
}

type RailRowProps = {
  label: string;
  children: ReactNode;
};

function RailRow(props: RailRowProps) {
  const { label, children } = props;

  return (
    <div className="flex gap-3">
      <dt className="w-27 shrink-0 truncate text-text-secondary">{label}</dt>
      <dd className="min-w-0 flex-1 wrap-anywhere">{children}</dd>
    </div>
  );
}

type ActionPillProps = {
  icon: IconName;
  tone: 'dark' | 'light';
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
};

function ActionPill(props: ActionPillProps) {
  const { icon, tone, disabled, onClick, children } = props;

  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className={cn(
        'flex h-7 items-center gap-1.5 rounded-full pr-3 pl-2 text-body font-medium outline-none focus-visible:ring-2 focus-visible:ring-text-link/60 disabled:opacity-40 [&_svg]:size-3.5',
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
