import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { Dialog } from '~/components/ui/dialog';
import { request } from '~/lib/http';
import { apiClient } from '~/lib/api-client';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { connectProviderOptions, telegramConnectOptions } from '~/queries/alerts';
import type { ConnectProvider } from '~/queries/alerts';
import { CONNECTED_MESSAGE, INTEGRATION_KINDS } from '~/utils/alerts';

const COPY: Record<ConnectProvider, { title: string; detail: string; permissions: string[]; action: string }> = {
  slack: {
    title: 'Post alerts to a Slack channel',
    detail: 'Sign in to Slack, then pick the channel in the same window.',
    permissions: ['See the names of public channels', 'Post messages in the channel you pick', 'Nothing else. Daroyan never reads your messages.'],
    action: 'Continue to Slack',
  },
  discord: {
    title: 'Post alerts to a Discord channel',
    detail: 'Discord asks for the server and the channel. That is the only step.',
    permissions: ['Post messages in the channel you pick', 'Nothing else. Daroyan never reads your messages.'],
    action: 'Continue to Discord',
  },
  telegram: {
    title: 'Send alerts to a Telegram chat',
    detail: 'Pick a group in Telegram and the Daroyan bot joins it. This window updates by itself.',
    permissions: ['Post messages in the chat you pick', 'Nothing else. The bot only reads its start command.'],
    action: 'Open Telegram',
  },
};

export type Connection = {
  url: string;
  state: string;
};

export function openConnection(provider: ConnectProvider, connection: Connection) {
  if (provider === 'telegram') {
    window.open(connection.url, '_blank', 'noopener');
    return;
  }
  const popup = window.open(connection.url, 'daroyan-connect', 'width=640,height=760');
  if (!popup) {
    window.location.href = connection.url;
  }
}

type OneClickConnectProps = {
  provider: ConnectProvider;
  started: Connection;
  onBack: () => void;
  onClose: () => void;
  onSaved?: (integration: Schemas['Integration']) => void;
};

export function OneClickConnect(props: OneClickConnectProps) {
  const { provider, started, onBack, onClose, onSaved } = props;

  const [error, setError] = useState<string>();
  const [isWaiting, setIsWaiting] = useState(true);
  const [connection, setConnection] = useState(started);
  const telegramState = provider === 'telegram' ? connection.state : undefined;
  const copy = COPY[provider];
  const info = INTEGRATION_KINDS[provider];

  const finish = async (integrationId: string) => {
    const integrations = await queryClient.fetchQuery({
      queryKey: ['integrations'],
      queryFn: () => request(apiClient.GET('/v1/integrations')),
      staleTime: 0,
    });
    const saved = integrations.find((integration) => integration.id === integrationId);
    queryClient.invalidateQueries({ queryKey: ['integrations'] });
    toast.success(`Connected ${saved?.name ?? info.label}`);
    if (saved) {
      onSaved?.(saved);
    }
    onClose();
  };

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (event.origin !== window.location.origin || event.data?.type !== CONNECTED_MESSAGE) {
        return;
      }
      setIsWaiting(false);
      if (event.data.integrationId) {
        void finish(event.data.integrationId);
      } else {
        setError(event.data.error || 'The connection did not finish');
      }
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  });

  const { data: telegram } = useQuery({
    ...telegramConnectOptions(telegramState ?? ''),
    enabled: !!telegramState,
  });
  useEffect(() => {
    if (telegram?.is_connected && telegram.integration) {
      void finish(telegram.integration.id);
    }
  }, [telegram?.is_connected]);

  const connect = useMutation({
    ...connectProviderOptions(),
    meta: { isSilent: true },
    onError: (failure) => setError(failure.message),
    onSuccess: (next) => {
      setError(undefined);
      setIsWaiting(true);
      setConnection(next);
      openConnection(provider, next);
    },
  });

  const directChatUrl = connection.url.replace('?startgroup=', '?start=');

  return (
    <>
      <Dialog.Header>
        <Dialog.Title>Connect {info.label}</Dialog.Title>
      </Dialog.Header>
      <Dialog.Body>
        <div className="flex items-center gap-3">
          <span className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-surface-fill">
            <Icon name={info.icon} className="size-5" />
          </span>
          <div className="flex min-w-0 flex-1 flex-col gap-0.5 text-body">
            <span className="font-medium">{copy.title}</span>
            <span className="text-text-secondary">{copy.detail}</span>
          </div>
        </div>
        <div className="flex flex-col gap-2 rounded-lg bg-surface-subtle p-3">
          <span className="text-caption font-medium text-text-tertiary">Daroyan will be able to</span>
          {copy.permissions.map((permission) => (
            <span key={permission} className="flex items-center gap-2 text-body">
              <Icon name="check" className="size-3.5" />
              {permission}
            </span>
          ))}
        </div>
        {isWaiting && !error && (
          <p role="status" className="flex items-center gap-2 text-body text-text-secondary">
            <Icon name="loader" className="animate-spin" />
            {provider === 'telegram' ? (
              <span>
                Waiting for the bot to join a chat.{' '}
                {directChatUrl && (
                  <a href={directChatUrl} target="_blank" rel="noreferrer" className="text-text-link hover:underline">
                    Use a private chat instead
                  </a>
                )}
              </span>
            ) : (
              `Finish in the ${info.label} window.`
            )}
          </p>
        )}
        {error && (
          <p role="alert" className="flex items-start gap-2 rounded-lg bg-tone-critical-bg px-3 py-2 text-body text-tone-critical-fg">
            <Icon name="triangle-alert" className="mt-0.5" />
            {error}
          </p>
        )}
      </Dialog.Body>
      <Dialog.Footer>
        <Button type="button" variant="secondary" className="w-fit" onClick={onBack}>
          Back
        </Button>
        <Button
          type="button"
          className="w-fit"
          disabled={connect.isPending}
          onClick={() => connect.mutate(provider)}
        >
          {connect.isPending ? <Icon name="loader" className="animate-spin" /> : <Icon name="external-link" />}
          {isWaiting ? 'Open Again' : copy.action}
        </Button>
      </Dialog.Footer>
    </>
  );
}
