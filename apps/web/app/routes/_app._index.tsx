import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import type { ReactNode } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { OverviewDashboard } from '~/components/overview/overview-dashboard';
import { Button } from '~/components/ui/button';
import { CodeBlock } from '~/components/ui/code-block';
import { PageBar } from '~/components/ui/page-bar';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { listIntegrationsOptions } from '~/queries/alerts';
import { listCronsOptions } from '~/queries/crons';
import { installTokenOptions } from '~/queries/install-tokens';
import { listServersOptions } from '~/queries/servers';
import { listUptimeChecksOptions } from '~/queries/uptime';
import { cn } from '~/utils/classname';

export function meta() {
  return [{ title: 'Overview · Daroyan' }];
}

function getGreeting(hour: number) {
  if (hour < 12) {
    return 'Good morning';
  }
  return hour < 18 ? 'Good afternoon' : 'Good evening';
}

const SKIPPED_KEY = 'daroyan.setup.skipped';

function readSkipped(): string[] {
  try {
    return JSON.parse(localStorage.getItem(SKIPPED_KEY) ?? '[]') as string[];
  } catch {
    return [];
  }
}

type Step = {
  key: string;
  title: string;
  description: string;
  isDone: boolean;
  action: ReactNode;
};

export default function OverviewPage() {
  const { data: servers } = useQuery({ ...listServersOptions(), refetchInterval: 5_000 });
  const { data: checks } = useQuery(listUptimeChecksOptions());
  const { data: crons } = useQuery(listCronsOptions());
  const { data: integrations } = useQuery(listIntegrationsOptions());
  const isAdmin = useIsAdmin();
  const [openKey, setOpenKey] = useState<string>();
  const [skipped, setSkipped] = useState(readSkipped);

  const setSkip = (keys: string[], isSkipped: boolean) => {
    const next = isSkipped ? [...new Set([...skipped, ...keys])] : skipped.filter((item) => !keys.includes(item));
    setSkipped(next);
    setOpenKey(undefined);
    try {
      localStorage.setItem(SKIPPED_KEY, JSON.stringify(next));
    } catch {}
  };

  const isLoaded = !!servers && !!checks && !!crons && !!integrations;
  const steps: Step[] = [
    {
      key: 'server',
      title: 'Install the agent on a server',
      description: 'Run this on a server. Daroyan shows its CPU, memory, disk and processes about 10 seconds later.',
      isDone: !!servers?.some((server) => server.last_seen_at),
      action: <InstallCommand />,
    },
    {
      key: 'check',
      title: 'Add an uptime check',
      description: 'Daroyan calls a URL or opens a port on a schedule, and tells you when it stops answering.',
      isDone: (checks?.length ?? 0) > 0,
      action: (
        <Button nativeButton={false} render={<Link to="/uptime/new" />} className="w-fit">
          <Icon name="plus" />
          New Check
        </Button>
      ),
    },
    {
      key: 'cron',
      title: 'Watch a cron',
      description: 'Your job pings Daroyan each time it runs. You hear about it when a ping is late, missing or fails.',
      isDone: (crons?.length ?? 0) > 0,
      action: (
        <Button nativeButton={false} render={<Link to="/crons/new" />} className="w-fit">
          <Icon name="plus" />
          New Cron
        </Button>
      ),
    },
    {
      key: 'channel',
      title: 'Connect Slack, Discord or Telegram',
      description: 'Alerts and incident updates go to the channels you connect.',
      isDone: (integrations?.length ?? 0) > 0,
      action: (
        <Button nativeButton={false} render={<Link to="/settings/integrations" />} className="w-fit">
          Connect a Channel
        </Button>
      ),
    },
  ];
  const isComplete = (step: Step) => step.isDone || skipped.includes(step.key);
  const doneCount = steps.filter(isComplete).length;

  if (isLoaded && (doneCount === steps.length || !isAdmin)) {
    return <OverviewDashboard servers={servers} checks={checks} crons={crons} />;
  }

  const activeKey = openKey ?? steps.find((step) => !isComplete(step))?.key;

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="house" title="Overview" />
      {isLoaded && (
        <div className="flex flex-col items-center gap-6 px-4 pt-7 pb-16">
          <div className="flex flex-col items-center gap-0.5 text-center text-title-page font-semibold">
            <p className="text-text-secondary">{getGreeting(new Date().getHours())}</p>
            <h1 className="text-balance text-text-primary">Let’s get Daroyan watching your stack</h1>
          </div>
          <section className="flex w-180 max-w-full flex-col gap-1 rounded-[20px] border border-border-ring bg-surface-default p-3 shadow-card">
            <div className="flex flex-col gap-1 px-2 pt-1 pb-2">
              <div className="flex items-center justify-between gap-3">
                <h2 className="text-title-section font-medium">Setup Guide</h2>
                <Button
                  variant="plain"
                  className="-my-1 w-fit"
                  onClick={() =>
                    setSkip(
                      steps.map((step) => step.key),
                      true
                    )
                  }
                >
                  Skip Setup
                </Button>
              </div>
              <p className="text-body text-text-secondary">Use these steps to start watching your servers, sites and jobs.</p>
              <div className="flex items-center gap-2 pt-1.5">
                <span className="text-small text-text-secondary">
                  {doneCount} of {steps.length} done
                </span>
                <span
                  role="progressbar"
                  aria-label="Setup progress"
                  aria-valuenow={doneCount}
                  aria-valuemin={0}
                  aria-valuemax={steps.length}
                  className="h-1 w-21 overflow-hidden rounded-[2px] bg-surface-panel"
                >
                  <span
                    className="block h-full rounded-[2px] bg-tone-success-fg transition-[width]"
                    style={{ width: `${(doneCount / steps.length) * 100}%` }}
                  />
                </span>
              </div>
            </div>
            {steps.map((step) => {
              const isOpen = step.key === activeKey;
              const isSkipped = !step.isDone && skipped.includes(step.key);
              return (
                <div key={step.key} className={cn('flex gap-3 rounded-xl px-2 py-2.5', isOpen && 'bg-surface-panel')}>
                  <Icon
                    name={step.isDone || isSkipped ? 'circle-check-fill' : 'circle-dashed'}
                    className={cn('mt-0.5 shrink-0', step.isDone ? 'text-tone-success-fg' : 'text-icon-secondary')}
                  />
                  <div className="flex min-w-0 flex-1 flex-col gap-1">
                    <button
                      type="button"
                      aria-expanded={isOpen}
                      onClick={() => setOpenKey(step.key)}
                      className={cn(
                        'w-fit rounded-xs text-left text-body font-medium outline-none focus-visible:ring-2 focus-visible:ring-text-link/40',
                        step.isDone || isSkipped ? 'text-text-secondary line-through' : 'text-text-primary'
                      )}
                    >
                      {step.title}
                    </button>
                    {isOpen && (
                      <div className="flex flex-col gap-2.5">
                        <p className="text-body text-text-secondary">{step.description}</p>
                        {step.isDone ? (
                          <p className="flex items-center gap-1.5 text-small text-tone-success-fg">
                            <Icon name="check" className="size-3.5" />
                            Done
                          </p>
                        ) : (
                          <div className="flex flex-wrap items-center gap-2">
                            {step.action}
                            {isSkipped ? (
                              <Button variant="plain" className="w-fit" onClick={() => setSkip([step.key], false)}>
                                Undo Skip
                              </Button>
                            ) : (
                              <Button variant="plain" className="w-fit" onClick={() => setSkip([step.key], true)}>
                                Skip
                              </Button>
                            )}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                </div>
              );
            })}
          </section>
        </div>
      )}
    </div>
  );
}

function InstallCommand() {
  const { data: installToken } = useQuery(installTokenOptions(0, 1));
  const token = installToken?.token ?? '…';
  const command = `curl -fsSL ${window.location.origin}/install.sh | \\\n  sudo DAROYAN_API_URL=${window.location.origin} DAROYAN_TOKEN=${token} sh`;

  return (
    <div className="flex flex-col gap-2.5">
      <CodeBlock label="Install command" code={command} language="shellscript" />
      <p className="flex items-center gap-1.5 text-small text-text-secondary">
        <Icon name="clock" className="size-3.5" />
        Waiting for the first report
      </p>
    </div>
  );
}
