import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { OneClickConnect, openConnection } from '~/components/alerts/one-click-connect';
import type { Connection } from '~/components/alerts/one-click-connect';
import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { Button } from '~/components/ui/button';
import { Dialog } from '~/components/ui/dialog';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import {
  connectProviderOptions,
  createIntegrationOptions,
  integrationProvidersOptions,
  testIntegrationConfigOptions,
  testIntegrationOptions,
  updateIntegrationOptions,
} from '~/queries/alerts';
import type { ConnectProvider } from '~/queries/alerts';
import type { IntegrationKind } from '~/utils/alerts';
import { INTEGRATION_KINDS, isIntegrationKind, joinNames } from '~/utils/alerts';
import { cn } from '~/utils/classname';

const KIND_ORDER: IntegrationKind[] = ['slack', 'discord', 'telegram', 'webhook'];

const SIGN_IN_KINDS: ConnectProvider[] = ['slack', 'discord', 'telegram'];

const SETUP_ENV: Record<ConnectProvider, string> = {
  slack: 'SLACK_CLIENT_ID',
  discord: 'DISCORD_CLIENT_ID',
  telegram: 'TELEGRAM_BOT_TOKEN',
};

type TestResult = { ok: true; message: string } | { ok: false; message: string };

type IntegrationDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  integration?: Schemas['Integration'];
  onSaved?: (integration: Schemas['Integration']) => void;
};

export function IntegrationDialog(props: IntegrationDialogProps) {
  const { open, onOpenChange, integration, onSaved } = props;

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Backdrop />
        <Dialog.Popup>
          {open && (
            <IntegrationDialogContent
              integration={integration}
              onClose={() => onOpenChange(false)}
              onSaved={onSaved}
            />
          )}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

type IntegrationDialogContentProps = {
  integration?: Schemas['Integration'];
  onClose: () => void;
  onSaved?: (integration: Schemas['Integration']) => void;
};

const DESCRIPTIONS: Record<IntegrationKind, string> = {
  slack: 'Sign in to Slack and pick a channel',
  discord: 'Sign in to Discord and pick a channel',
  telegram: 'Add the Daroyan bot to a chat',
  webhook: 'Send JSON to any URL',
};

function IntegrationDialogContent(props: IntegrationDialogContentProps) {
  const { integration, onClose, onSaved } = props;

  const { data: providers } = useQuery(integrationProvidersOptions());
  const isAvailable = (item: IntegrationKind) => item === 'webhook' || !!providers?.[item];
  const [picked, setPicked] = useState<IntegrationKind>();
  const [step, setStep] = useState<{ kind: 'webhook' } | { kind: ConnectProvider; started: Connection }>();
  const [error, setError] = useState<string>();
  const selected = picked ?? KIND_ORDER.find(isAvailable) ?? 'webhook';
  const missing = SIGN_IN_KINDS.filter((item) => providers && !providers[item]);

  const connect = useMutation({
    ...connectProviderOptions(),
    meta: { isSilent: true },
    onError: (failure) => setError(failure.message),
    onSuccess: (started, provider) => {
      openConnection(provider, started);
      setStep({ kind: provider, started });
    },
  });

  if (integration) {
    return integration.kind === 'webhook' ? (
      <WebhookForm integration={integration} onClose={onClose} onSaved={onSaved} />
    ) : (
      <RenameForm integration={integration} onClose={onClose} onSaved={onSaved} />
    );
  }

  if (step?.kind === 'webhook') {
    return <WebhookForm onBack={() => setStep(undefined)} onClose={onClose} onSaved={onSaved} />;
  }
  if (step) {
    return (
      <OneClickConnect
        provider={step.kind}
        started={step.started}
        onBack={() => setStep(undefined)}
        onClose={onClose}
        onSaved={onSaved}
      />
    );
  }

  const isSignIn = selected !== 'webhook';

  return (
    <>
      <Dialog.Header>
        <Dialog.Title>Add Integration</Dialog.Title>
      </Dialog.Header>
      <Dialog.Body>
        <Dialog.Description>Pick where alert messages go.</Dialog.Description>
        <div
          role="radiogroup"
          aria-label="Integration Kind"
          className="flex flex-col overflow-hidden rounded-2xl border border-border-ring [&>*+*]:border-t [&>*+*]:border-border-row"
        >
          {KIND_ORDER.map((item) => (
            <KindRow
              key={item}
              icon={INTEGRATION_KINDS[item].icon}
              label={INTEGRATION_KINDS[item].label}
              description={isAvailable(item) ? DESCRIPTIONS[item] : 'Sign-in is not set up on this server'}
              isSelected={selected === item}
              isDisabled={!isAvailable(item)}
              onSelect={() => {
                setPicked(item);
                setError(undefined);
              }}
            />
          ))}
          <KindRow icon="mail" label="Email" description="Send to an address" isDisabled badge="Soon" />
        </div>
        {missing.length > 0 && (
          <p className="flex items-start gap-2 text-small text-text-tertiary">
            <Icon name="info" className="mt-0.5 size-3.5" />
            {joinNames(missing.map((item) => INTEGRATION_KINDS[item].label))}{' '}
            {missing.length === 1 ? 'needs' : 'need'} sign-in set up on the server. Set{' '}
            {joinNames(missing.map((item) => SETUP_ENV[item]))}.
          </p>
        )}
        {providers?.slack_manifest_url && (
          <p className="flex items-start gap-2 text-small text-text-tertiary">
            <Icon name="info" className="mt-0.5 size-3.5" />
            <span>
              <a href={providers.slack_manifest_url} target="_blank" rel="noreferrer" className="text-text-link hover:underline">
                Create the Slack app
              </a>{' '}
              with everything filled in, then copy its Client ID and Client Secret. Slack can't prefill the icon, so{' '}
              <a href="/icon-512.png" download="daroyan-icon.png" className="text-text-link hover:underline">
                download it
              </a>{' '}
              and upload it under Basic Information.
            </span>
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
        <Button variant="secondary" className="w-fit" onClick={onClose}>
          Cancel
        </Button>
        <Button
          className="w-fit"
          disabled={!isAvailable(selected) || connect.isPending}
          onClick={() => (isSignIn ? connect.mutate(selected) : setStep({ kind: 'webhook' }))}
        >
          {connect.isPending ? (
            <Icon name="loader" className="animate-spin" />
          ) : (
            isSignIn && <Icon name="external-link" />
          )}
          {isSignIn ? `Continue to ${INTEGRATION_KINDS[selected].label}` : 'Continue'}
        </Button>
      </Dialog.Footer>
    </>
  );
}

type KindRowProps = {
  icon: IconName;
  label: string;
  description: string;
  isSelected?: boolean;
  isDisabled?: boolean;
  badge?: string;
  onSelect?: () => void;
};

function KindRow(props: KindRowProps) {
  const { icon, label, description, isSelected = false, isDisabled = false, badge, onSelect } = props;

  return (
    <button
      type="button"
      role="radio"
      aria-checked={isSelected}
      disabled={isDisabled}
      onClick={onSelect}
      className="flex items-center gap-3 px-3.5 py-3 text-left outline-none hover:bg-surface-subtle focus-visible:bg-surface-subtle disabled:pointer-events-none aria-checked:bg-surface-subtle"
    >
      <Icon name={icon} className={cn('size-5', isDisabled && 'opacity-50')} />
      <span className={cn('flex min-w-0 flex-1 flex-col', isDisabled && 'text-text-tertiary')}>
        <span className="truncate text-body font-medium">{label}</span>
        <span className={cn('truncate text-small', !isDisabled && 'text-text-secondary')}>{description}</span>
      </span>
      {badge && (
        <span className="flex h-5.5 shrink-0 items-center rounded-sm bg-tone-neutral-bg px-2 text-caption font-medium text-tone-neutral-fg">
          {badge}
        </span>
      )}
      {isSelected && <Icon name="check" />}
    </button>
  );
}

function useSavedHandler(options: {
  isEditing: boolean;
  onClose: () => void;
  onSaved?: (integration: Schemas['Integration']) => void;
}) {
  const { isEditing, onClose, onSaved } = options;

  return (saved: Schemas['Integration']) => {
    queryClient.invalidateQueries({ queryKey: ['integrations'] });
    toast.success(isEditing ? `Saved ${saved.name}` : `Added ${saved.name}`);
    onSaved?.(saved);
    onClose();
  };
}

type RenameFormProps = {
  integration: Schemas['Integration'];
  onClose: () => void;
  onSaved?: (integration: Schemas['Integration']) => void;
};

function RenameForm(props: RenameFormProps) {
  const { integration, onClose, onSaved } = props;

  const [name, setName] = useState(integration.name);
  const onDone = useSavedHandler({ isEditing: true, onClose, onSaved });
  const update = useMutation({
    ...updateIntegrationOptions(),
    onSuccess: onDone,
    meta: { errorMessage: 'The integration could not be saved' },
  });
  const label = isIntegrationKind(integration.kind) ? INTEGRATION_KINDS[integration.kind].label : integration.kind;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (name.trim()) {
          update.mutate({ integrationId: integration.id, body: { name: name.trim() } });
        }
      }}
    >
      <Dialog.Header>
        <Dialog.Title>Edit {label}</Dialog.Title>
      </Dialog.Header>
      <Dialog.Body>
        <FieldRow
          label="Name"
          hint={`Shown in the integration list and in alert rules. To post somewhere else, connect ${label} again.`}
        >
          {({ inputId, hintId }) => (
            <Input
              id={inputId}
              aria-describedby={hintId}
              value={name}
              onValueChange={setName}
              maxLength={100}
              required
              autoFocus
            />
          )}
        </FieldRow>
      </Dialog.Body>
      <Dialog.Footer>
        <Button type="button" variant="secondary" className="w-fit" onClick={onClose}>
          Cancel
        </Button>
        <Button type="submit" className="w-fit" disabled={!name.trim() || update.isPending}>
          {update.isPending && <Icon name="loader" className="animate-spin" />}
          Save
        </Button>
      </Dialog.Footer>
    </form>
  );
}

type WebhookFormProps = {
  integration?: Schemas['Integration'];
  onBack?: () => void;
  onClose: () => void;
  onSaved?: (integration: Schemas['Integration']) => void;
};

function WebhookForm(props: WebhookFormProps) {
  const { integration, onBack, onClose, onSaved } = props;

  const [name, setName] = useState(integration?.name ?? '');
  const [url, setUrl] = useState('');
  const [result, setResult] = useState<TestResult>();

  const isEditing = !!integration;
  const config = url.trim() ? { kind: 'webhook' as const, url: url.trim() } : undefined;

  const onTested = () => setResult({ ok: true, message: 'Test message sent to the URL.' });
  const onTestFailed = (error: Error) => setResult({ ok: false, message: error.message });
  const testConfig = useMutation({
    ...testIntegrationConfigOptions(),
    onSuccess: onTested,
    onError: onTestFailed,
    meta: { isSilent: true },
  });
  const testSaved = useMutation({
    ...testIntegrationOptions(),
    onSuccess: onTested,
    onError: onTestFailed,
    meta: { isSilent: true },
  });

  const onDone = useSavedHandler({ isEditing, onClose, onSaved });
  const create = useMutation({
    ...createIntegrationOptions(),
    onSuccess: onDone,
    meta: { errorMessage: 'The integration could not be added' },
  });
  const update = useMutation({
    ...updateIntegrationOptions(),
    onSuccess: onDone,
    meta: { errorMessage: 'The integration could not be saved' },
  });

  const isTesting = testConfig.isPending || testSaved.isPending;
  const isSaving = create.isPending || update.isPending;
  const canSave = !!name.trim() && (isEditing || !!config);

  const sendTest = () => {
    setResult(undefined);
    if (config) {
      testConfig.mutate(config);
    } else if (integration) {
      testSaved.mutate({ integrationId: integration.id });
    }
  };

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (!canSave) {
          return;
        }
        if (integration) {
          update.mutate({ integrationId: integration.id, body: { name: name.trim(), config } });
        } else if (config) {
          create.mutate({ name: name.trim(), config });
        }
      }}
    >
      <Dialog.Header>
        <Dialog.Title>{isEditing ? 'Edit Webhook' : 'Add Webhook'}</Dialog.Title>
      </Dialog.Header>
      <Dialog.Body>
        <FieldRow label="Name" hint="Shown in the integration list and in alert rules.">
          {({ inputId, hintId }) => (
            <Input
              id={inputId}
              aria-describedby={hintId}
              value={name}
              onValueChange={setName}
              placeholder="PagerDuty Bridge"
              maxLength={100}
              required
              autoFocus
            />
          )}
        </FieldRow>
        <FieldRow
          label="URL"
          hint={`Daroyan POSTs JSON with event, title, body, url and data. It must answer within 10 seconds.${isEditing ? ' Leave it empty to keep the current one.' : ''}`}
        >
          {({ inputId, hintId }) => (
            <Input
              id={inputId}
              aria-describedby={hintId}
              type="url"
              autoComplete="off"
              value={url}
              onValueChange={(value) => {
                setUrl(value);
                setResult(undefined);
              }}
              placeholder={integration?.target ?? 'https://example.com/hooks/daroyan'}
              required={!isEditing}
            />
          )}
        </FieldRow>
        {result && (
          <p
            role="status"
            className={cn(
              'flex items-start gap-2 rounded-lg px-3 py-2 text-body',
              result.ok ? 'bg-tone-success-bg text-tone-success-fg' : 'bg-tone-critical-bg text-tone-critical-fg'
            )}
          >
            <Icon name={result.ok ? 'check' : 'triangle-alert'} className="mt-0.5" />
            {result.message}
          </p>
        )}
      </Dialog.Body>
      <Dialog.Footer>
        <Button
          type="button"
          variant="secondary"
          className="mr-auto w-fit"
          disabled={isTesting || (!config && !isEditing)}
          onClick={sendTest}
        >
          {isTesting && <Icon name="loader" className="animate-spin" />}
          Send Test
        </Button>
        <Button type="button" variant="secondary" className="w-fit" onClick={onBack ?? onClose}>
          {onBack ? 'Back' : 'Cancel'}
        </Button>
        <Button type="submit" className="w-fit" disabled={!canSave || isSaving}>
          {isSaving && <Icon name="loader" className="animate-spin" />}
          Save
        </Button>
      </Dialog.Footer>
    </form>
  );
}
