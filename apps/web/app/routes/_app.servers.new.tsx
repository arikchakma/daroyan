import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { Navigate } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { PageBar } from '~/components/ui/page-bar';
import { CodeBlock } from '~/components/ui/code-block';
import { Select } from '~/components/ui/select';
import { installTokenOptions, revokeInstallTokenOptions } from '~/queries/install-tokens';
import { listServersOptions } from '~/queries/servers';

const INSTALL_TOKEN_TTL_MS = 24 * 3_600_000;

const MAX_USES_ITEMS = [1, 5, 25, 100].map((count) => ({
  value: String(count),
  label: count === 1 ? '1 server' : `${count} servers`,
}));

const CONFIG_EXAMPLE = `server:
  name: worker-2
  labels: [production, worker]

processes:
  - name: Inboxing Worker
    systemd: inboxing-worker
    restart:
      attempts: 3
      within: 10m
  - docker: redis-cache

ports:
  - host: redis-1
    port: 6379`;

function maskToken(token: string) {
  return `${token.slice(0, 13)}…${token.slice(-3)}`;
}

export default function AddServerPage() {
  const [tokenVersion, setTokenVersion] = useState(0);
  const [maxUses, setMaxUses] = useState(1);
  const { data: installToken, isFetching } = useQuery(installTokenOptions(tokenVersion, maxUses));
  const { mutate: revokeInstallToken } = useMutation({
    ...revokeInstallTokenOptions(),
    meta: { errorMessage: 'The old install token could not be revoked' },
  });

  const replaceToken = (nextMaxUses: number) => {
    if (installToken) {
      revokeInstallToken({ tokenId: installToken.id });
    }
    setMaxUses(nextMaxUses);
    setTokenVersion(tokenVersion + 1);
  };
  const { data: servers } = useQuery({
    ...listServersOptions(),
    refetchInterval: 3_000,
    refetchIntervalInBackground: true,
  });

  const tokenCreatedAt = installToken
    ? new Date(installToken.expires_at).getTime() - INSTALL_TOKEN_TTL_MS
    : Infinity;
  const newServer = servers?.find((server) => new Date(server.created_at).getTime() >= tokenCreatedAt);
  if (newServer?.last_seen_at) {
    return <Navigate to={`/servers/${newServer.id}`} replace />;
  }

  const token = installToken?.token ?? '';
  const installCommand = `curl -fsSL ${window.location.origin}/install.sh | \\\n  sudo DAROYAN_API_URL=${window.location.origin} DAROYAN_TOKEN=${token} sh`;
  const hoursLeft = installToken
    ? Math.max(Math.round((new Date(installToken.expires_at).getTime() - Date.now()) / 3_600_000), 0)
    : 24;

  return (
    <div className="flex min-h-full flex-col bg-surface-page">
      <PageBar backTo="/servers" icon="server" title="Servers">
        <Button
          nativeButton={false}
          render={<a href="https://github.com/arikchakma/daroyan" target="_blank" rel="noreferrer" />}
          variant="secondary"
          className="w-fit"
        >
          <Icon name="external-link" />
          Read the Docs
        </Button>
      </PageBar>

      <div className="flex flex-1 flex-col justify-center gap-10 px-10 pt-3 pb-12 xl:flex-row">
        <div className="flex w-full max-w-200 min-w-0 flex-col gap-6">
          <div className="flex flex-col gap-1">
            <h1 className="text-title-page font-semibold text-text-primary">Add Server</h1>
            <p className="text-body text-text-secondary">
              Install the Daroyan agent. It reports metrics and runs your checks every 10 seconds.
            </p>
          </div>

          <Step
            title="1. Install the Agent"
            description="Run this on the server as root. It works on Ubuntu, Debian and Alpine, on x86_64 and arm64."
          >
            <CodeBlock label="Install command" code={installCommand} language="shellscript" />
          </Step>

          <Step
            title="2. Pick What to Watch"
            description="The agent reads /etc/daroyan/agent.yml. Host metrics need no config. Watch a systemd unit, a Docker container or a process by name, and the ports you care about."
          >
            <CodeBlock label="Agent config" code={CONFIG_EXAMPLE} language="yaml" />
          </Step>

          <Step title="3. Wait for the First Report">
            <div className="flex items-center gap-3 rounded-2xl border border-border-ring bg-surface-default px-4 py-3.5 shadow-card">
              <Icon name="loader" className="animate-spin text-icon-secondary" />
              <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                <p className="text-body font-medium text-text-primary">
                  {newServer ? `${newServer.name} enrolled, waiting for its first report` : 'Waiting for the agent'}
                </p>
                <p className="text-body text-text-secondary">
                  This page moves on as soon as the agent connects. Outbound HTTPS on port 443 must be open.
                </p>
              </div>
              <Badge tone="neutral">{newServer ? 'Enrolled' : 'Not Connected'}</Badge>
            </div>
          </Step>
        </div>

        <aside className="flex w-full shrink-0 flex-col gap-4 pt-2 xl:w-65">
          <div className="flex flex-col gap-2 border-b border-border-row pb-4">
            <h2 className="text-body font-medium text-text-primary">Install Token</h2>
            <RailRow label="Token">
              <span className="font-mono text-code-small text-text-primary">{token ? maskToken(token) : '…'}</span>
            </RailRow>
            <RailRow label="Expires">In {hoursLeft} hours</RailRow>
            <RailRow label="Works for">
              <Select.Root
                items={MAX_USES_ITEMS}
                value={String(maxUses)}
                onValueChange={(next) => next && replaceToken(Number(next))}
                disabled={isFetching}
              >
                <Select.Trigger aria-label="Servers this token can add" className="h-7 w-32">
                  <Select.Value />
                </Select.Trigger>
                <Select.Portal>
                  <Select.Positioner>
                    <Select.Popup>
                      {MAX_USES_ITEMS.map((item) => (
                        <Select.Item key={item.value} value={item.value}>
                          {item.label}
                        </Select.Item>
                      ))}
                    </Select.Popup>
                  </Select.Positioner>
                </Select.Portal>
              </Select.Root>
            </RailRow>
            <Button
              variant="secondary"
              className="w-fit"
              disabled={isFetching}
              onClick={() => replaceToken(maxUses)}
            >
              <Icon name="refresh" className={isFetching ? 'animate-spin' : undefined} />
              Roll Token
            </Button>
          </div>
          <div className="flex flex-col gap-2 border-b border-border-row pb-4">
            <h2 className="text-body font-medium text-text-primary">Requirements</h2>
            <RailRow label="OS">Linux, kernel 4.18+</RailRow>
            <RailRow label="Memory">About 40 MB</RailRow>
            <RailRow label="Network">Outbound 443</RailRow>
            <RailRow label="Runs as">daroyan user</RailRow>
          </div>
        </aside>
      </div>
    </div>
  );
}

type StepProps = {
  title: string;
  description?: string;
  children: React.ReactNode;
};

function Step(props: StepProps) {
  const { title, description, children } = props;

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-col gap-0.5">
        <h2 className="text-title-section font-medium text-text-primary">{title}</h2>
        {description && <p className="text-body text-text-secondary">{description}</p>}
      </div>
      {children}
    </section>
  );
}

type RailRowProps = {
  label: string;
  children: React.ReactNode;
};

function RailRow(props: RailRowProps) {
  const { label, children } = props;

  return (
    <div className="flex items-start gap-3 text-body">
      <span className="flex-1 text-text-secondary">{label}</span>
      <span className="text-right text-text-primary">{children}</span>
    </div>
  );
}
