import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { Link, Navigate } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { PageBar } from '~/components/ui/page-bar';
import { CodeBlock } from '~/components/ui/code-block';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import {
  EXPIRES_ITEMS,
  InstallKeySelect,
  MAX_USES_ITEMS,
} from '~/components/servers/install-key-select';
import { ONE_OFF_LIMITS, installTokenOptions, revokeInstallTokenOptions } from '~/queries/install-tokens';
import type { InstallTokenLimits } from '~/queries/install-tokens';
import { listServersOptions } from '~/queries/servers';

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
  const [limits, setLimits] = useState(ONE_OFF_LIMITS);
  const [isRolling, setIsRolling] = useState(false);
  const { data: installToken, isFetching } = useQuery(installTokenOptions(tokenVersion, limits));
  const { mutate: revokeInstallToken } = useMutation({
    ...revokeInstallTokenOptions(),
    meta: { errorMessage: 'The old install token could not be revoked' },
  });

  const replaceToken = (nextLimits: InstallTokenLimits) => {
    if (installToken) {
      revokeInstallToken({ tokenId: installToken.id });
    }
    setLimits(nextLimits);
    setTokenVersion(tokenVersion + 1);
  };
  const { data: servers } = useQuery({
    ...listServersOptions(),
    refetchInterval: 3_000,
    refetchIntervalInBackground: true,
  });

  const tokenCreatedAt = installToken ? new Date(installToken.created_at).getTime() : Infinity;
  const newServer = servers?.find((server) => new Date(server.created_at).getTime() >= tokenCreatedAt);
  if (newServer?.last_seen_at) {
    return <Navigate to={`/servers/${newServer.id}`} replace />;
  }

  const token = installToken?.token ?? '';
  const installCommand = `curl -fsSL ${window.location.origin}/install.sh | \\\n  sudo DAROYAN_API_URL=${window.location.origin} DAROYAN_TOKEN=${token} sh`;

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
            <RailRow label="Expires">
              <InstallKeySelect
                label="When this token expires"
                items={EXPIRES_ITEMS}
                value={limits.expires_in_hours}
                onValueChange={(expires_in_hours) => replaceToken({ ...limits, expires_in_hours })}
                disabled={isFetching}
                className="h-7 w-32"
              />
            </RailRow>
            <RailRow label="Works for">
              <InstallKeySelect
                label="Servers this token can add"
                items={MAX_USES_ITEMS}
                value={limits.max_uses}
                onValueChange={(max_uses) => replaceToken({ ...limits, max_uses })}
                disabled={isFetching}
                className="h-7 w-32"
              />
            </RailRow>
            <Link to="/settings/agents" className="text-body text-text-secondary underline underline-offset-2 hover:text-text-primary">
              Reuse a key from Settings → Agents
            </Link>
            <Button
              variant="secondary"
              className="w-fit"
              disabled={isFetching}
              onClick={() => setIsRolling(true)}
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

      <ConfirmDialog
        open={isRolling}
        onOpenChange={setIsRolling}
        title="Roll the Install Token?"
        description="The command above stops working right away. Servers that already enrolled keep reporting."
        confirmLabel="Roll Token"
        isPending={false}
        onConfirm={() => {
          setIsRolling(false);
          replaceToken(limits);
        }}
      />
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
