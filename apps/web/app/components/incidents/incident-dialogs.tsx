import { useMutation } from '@tanstack/react-query';
import type { JSONContent } from '@tiptap/core';
import { useId, useState } from 'react';
import type { ReactNode } from 'react';

import {
  AffectedComponentsCard,
  OptionSelect,
  Segmented,
  useIncidentDirectory,
} from '~/components/incidents/incident-form';
import type { AffectedComponent } from '~/components/incidents/incident-form';
import { Icon } from '~/components/icons/icon';
import { RichTextEditor } from '~/components/rich-text/rich-text-editor';
import { Button } from '~/components/ui/button';
import { Dialog } from '~/components/ui/dialog';
import { Input } from '~/components/ui/input';
import { Switch } from '~/components/ui/switch';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import {
  editIncidentUpdateOptions,
  postIncidentUpdateOptions,
  toIncidentInput,
  updateIncidentOptions,
} from '~/queries/incidents';
import type { IncidentInput } from '~/queries/incidents';
import { getComponentStatusMeta, isMaintenance, SEVERITY_META } from '~/utils/incidents';
import type { Incident, IncidentUpdate } from '~/utils/incidents';
import { hasText, toDocument } from '~/utils/rich-text';

function invalidate() {
  return queryClient.invalidateQueries({ queryKey: ['incidents'] });
}

type DialogFrameProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: ReactNode;
  submitLabel: ReactNode;
  isPending: boolean;
  onSubmit: () => void;
  className?: string;
  children: ReactNode;
};

function DialogFrame(props: DialogFrameProps) {
  const { open, onOpenChange, title, description, submitLabel, isPending, onSubmit, className, children } = props;

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Backdrop />
        <Dialog.Popup className={className}>
          <Dialog.Header>
            <Dialog.Title>{title}</Dialog.Title>
          </Dialog.Header>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              onSubmit();
            }}
          >
            <Dialog.Body>
              {description && <Dialog.Description className="text-pretty">{description}</Dialog.Description>}
              {children}
            </Dialog.Body>
            <Dialog.Footer>
              <Dialog.Close render={<Button variant="secondary" className="w-fit" disabled={isPending} />}>
                Cancel
              </Dialog.Close>
              <Button type="submit" className="w-fit" disabled={isPending}>
                {isPending && <Icon name="loader" className="animate-spin" />}
                {submitLabel}
              </Button>
            </Dialog.Footer>
          </form>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

type SwitchLineProps = {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onCheckedChange: (checked: boolean) => void;
};

function SwitchLine(props: SwitchLineProps) {
  const { label, checked, disabled = false, onCheckedChange } = props;

  const labelId = useId();

  return (
    <label className="flex items-center gap-2 text-body">
      <Switch.Root aria-labelledby={labelId} checked={checked} disabled={disabled} onCheckedChange={onCheckedChange}>
        <Switch.Thumb />
      </Switch.Root>
      <span id={labelId}>{label}</span>
    </label>
  );
}

const EDITOR_BOX_CLASS_NAME = 'overflow-hidden rounded-xl border border-border-ring bg-surface-default';

type ResolveDialogProps = {
  incident: Incident;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  shownOn: string;
  channelNames: string;
};

export function ResolveDialog(props: ResolveDialogProps) {
  const { incident, open, onOpenChange, shownOn, channelNames } = props;

  const maintenance = isMaintenance(incident);
  const [body, setBody] = useState<JSONContent | null>(null);
  const [isPublic, setIsPublic] = useState(true);
  const [notify, setNotify] = useState(true);
  const hasPages = incident.status_pages.some((page) => page.is_shown);
  const hasChannels = incident.channel_ids.length > 0;

  const post = useMutation({
    ...postIncidentUpdateOptions(),
    meta: { errorMessage: maintenance ? 'The maintenance could not be completed' : 'The incident could not be resolved' },
    onSuccess: () => {
      invalidate();
      toast.success(maintenance ? 'Maintenance completed' : 'Incident resolved');
      setBody(null);
      onOpenChange(false);
    },
  });

  return (
    <DialogFrame
      open={open}
      onOpenChange={onOpenChange}
      title={maintenance ? 'Complete Maintenance' : 'Resolve Incident'}
      description={`${incident.title}. The closing message goes on the timeline, and to the status pages and channels you keep on.`}
      submitLabel={
        <>
          <Icon name="check" />
          {maintenance ? 'Complete' : 'Resolve'}
        </>
      }
      isPending={post.isPending}
      className="w-140"
      onSubmit={() => {
        if (!body || !hasText(body)) {
          toast.error('Write a closing message');
          return;
        }
        post.mutate({
          incidentId: incident.id,
          body: {
            kind: 'update',
            status: maintenance ? 'completed' : 'resolved',
            body,
            is_public: isPublic && hasPages,
            notify: notify && hasChannels,
          },
        });
      }}
    >
      <div className="flex flex-col gap-1.5">
        <span className="text-body font-medium">Closing Message</span>
        <RichTextEditor
          autoFocus
          label="Closing message"
          placeholder="Say what fixed it and whether anything else is needed."
          content={body}
          onChange={setBody}
          className={EDITOR_BOX_CLASS_NAME}
        />
      </div>
      {incident.components.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <span className="text-body font-medium">Components Go Back to Their Monitor Status</span>
          <ul className="flex flex-col overflow-hidden rounded-xl border border-border-ring [&>*+*]:border-t [&>*+*]:border-border-row">
            {incident.components.map((component) => (
              <li key={`${component.kind}-${component.id}`} className="flex items-center gap-3 px-3 py-2.5 text-body">
                <span className="min-w-0 flex-1 truncate font-medium">{component.name}</span>
                <span className="flex shrink-0 items-center gap-1.5 text-small text-text-secondary">
                  {getComponentStatusMeta(component.status).label}
                  <Icon name="arrow-right" className="size-3.5" />
                  Monitor status
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="flex flex-col gap-2.5">
        <SwitchLine
          label={hasPages ? `Show ${shownOn}` : 'No status page shows this'}
          checked={isPublic && hasPages}
          disabled={!hasPages}
          onCheckedChange={setIsPublic}
        />
        <SwitchLine
          label={hasChannels ? `Notify ${channelNames}` : 'No channels to notify'}
          checked={notify && hasChannels}
          disabled={!hasChannels}
          onCheckedChange={setNotify}
        />
      </div>
    </DialogFrame>
  );
}

type EditUpdateDialogProps = {
  incidentId: string;
  update: IncidentUpdate | undefined;
  onOpenChange: (open: boolean) => void;
};

export function EditUpdateDialog(props: EditUpdateDialogProps) {
  const { incidentId, update, onOpenChange } = props;

  const [body, setBody] = useState<JSONContent | null>(null);

  const edit = useMutation({
    ...editIncidentUpdateOptions(),
    meta: { errorMessage: 'The update could not be saved' },
    onSuccess: () => {
      invalidate();
      toast.success('Update saved');
      onOpenChange(false);
    },
  });

  return (
    <DialogFrame
      open={!!update}
      onOpenChange={(open) => {
        setBody(null);
        onOpenChange(open);
      }}
      title={update?.kind === 'note' ? 'Edit Note' : 'Edit Update'}
      description={
        update?.is_public
          ? 'Status pages show the new text right away. Channels are not notified again.'
          : 'Only your team sees this.'
      }
      submitLabel="Save"
      isPending={edit.isPending}
      className="w-140"
      onSubmit={() => {
        const next = body ?? toDocument(update?.body);
        if (!update || !next || !hasText(next)) {
          toast.error('The update cannot be empty');
          return;
        }
        edit.mutate({ incidentId, updateId: update.id, body: { body: next } });
      }}
    >
      {update && (
        <RichTextEditor
          autoFocus
          label="Update"
          placeholder="Write an update…"
          content={toDocument(update.body)}
          onChange={setBody}
          className={EDITOR_BOX_CLASS_NAME}
        />
      )}
    </DialogFrame>
  );
}

function useIncidentSave(onDone: () => void, success: string) {
  return useMutation({
    ...updateIncidentOptions(),
    meta: { errorMessage: 'The incident could not be saved' },
    onSuccess: () => {
      invalidate();
      toast.success(success);
      onDone();
    },
  });
}

type IncidentDialogProps = {
  incident: Incident;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function EditComponentsDialog(props: IncidentDialogProps) {
  const { incident, open, onOpenChange } = props;

  const directory = useIncidentDirectory();
  const [components, setComponents] = useState<AffectedComponent[] | null>(null);
  const current = components ?? toIncidentInput(incident).components ?? [];
  const save = useIncidentSave(() => onOpenChange(false), 'Components saved');

  return (
    <DialogFrame
      open={open}
      onOpenChange={(next) => {
        setComponents(null);
        onOpenChange(next);
      }}
      title="Edit Components"
      description="Their status shows on your status pages until the incident is resolved."
      submitLabel="Save"
      isPending={save.isPending}
      className="w-140"
      onSubmit={() =>
        save.mutate({ incidentId: incident.id, body: { ...toIncidentInput(incident), components: current } })
      }
    >
      <AffectedComponentsCard
        directory={directory}
        components={current}
        onChange={setComponents}
        isMaintenance={isMaintenance(incident)}
      />
    </DialogFrame>
  );
}

const SEVERITY_OPTIONS = (['critical', 'major', 'minor'] as const).map((value) => ({
  value,
  label: SEVERITY_META[value]!.label,
}));

export function EditIncidentDialog(props: IncidentDialogProps) {
  const { incident, open, onOpenChange } = props;

  const directory = useIncidentDirectory();
  const [draft, setDraft] = useState<IncidentInput | null>(null);
  const current = draft ?? toIncidentInput(incident);
  const save = useIncidentSave(() => onOpenChange(false), 'Incident saved');
  const titleId = useId();

  return (
    <DialogFrame
      open={open}
      onOpenChange={(next) => {
        setDraft(null);
        onOpenChange(next);
      }}
      title={isMaintenance(incident) ? 'Edit Maintenance' : 'Edit Incident'}
      submitLabel="Save"
      isPending={save.isPending}
      onSubmit={() => {
        if (!current.title.trim()) {
          toast.error('Add a title');
          return;
        }
        save.mutate({ incidentId: incident.id, body: { ...current, title: current.title.trim() } });
      }}
    >
      <div className="flex flex-col gap-1.5">
        <label htmlFor={titleId} className="text-body font-medium">
          Title
        </label>
        <Input id={titleId} value={current.title} onValueChange={(title) => setDraft({ ...current, title })} />
      </div>
      {!isMaintenance(incident) && (
        <div className="flex flex-col gap-1.5">
          <span className="text-body font-medium">Severity</span>
          <Segmented
            label="Severity"
            value={(current.severity ?? 'major') as 'minor' | 'major' | 'critical'}
            options={SEVERITY_OPTIONS}
            onChange={(severity) => setDraft({ ...current, severity })}
          />
        </div>
      )}
      <div className="flex flex-col gap-1.5">
        <span className="text-body font-medium">Lead</span>
        <OptionSelect
          label="Lead"
          value={current.lead_id ?? null}
          placeholder="No lead"
          items={directory.users.map((user) => ({ value: user.id, label: user.name }))}
          onChange={(leadId) => setDraft({ ...current, lead_id: leadId })}
        />
      </div>
    </DialogFrame>
  );
}

export function AddResponderDialog(props: IncidentDialogProps) {
  const { incident, open, onOpenChange } = props;

  const directory = useIncidentDirectory();
  const [userId, setUserId] = useState<string | null>(null);
  const [role, setRole] = useState('Responder');
  const save = useIncidentSave(() => onOpenChange(false), 'Responder added');
  const roleId = useId();
  const taken = new Set([incident.lead?.id, ...incident.responders.map((responder) => responder.id)]);
  const available = directory.users.filter((user) => !taken.has(user.id));

  return (
    <DialogFrame
      open={open}
      onOpenChange={(next) => {
        setUserId(null);
        setRole('Responder');
        onOpenChange(next);
      }}
      title="Add Responder"
      description="Responders are listed on the incident. Only people who have signed in to Daroyan can be added."
      submitLabel="Add"
      isPending={save.isPending}
      onSubmit={() => {
        if (!userId || !role.trim()) {
          toast.error('Pick a person and a role');
          return;
        }
        const input = toIncidentInput(incident);
        save.mutate({
          incidentId: incident.id,
          body: { ...input, responders: [...(input.responders ?? []), { user_id: userId, role: role.trim() }] },
        });
      }}
    >
      <div className="flex flex-col gap-1.5">
        <span className="text-body font-medium">Person</span>
        <OptionSelect
          label="Person"
          value={userId}
          placeholder={available.length === 0 ? 'Everyone is already on it' : 'Pick someone'}
          items={available.map((user) => ({ value: user.id, label: user.name }))}
          onChange={setUserId}
        />
      </div>
      <div className="flex flex-col gap-1.5">
        <label htmlFor={roleId} className="text-body font-medium">
          Role
        </label>
        <Input id={roleId} value={role} maxLength={40} onValueChange={setRole} />
      </div>
    </DialogFrame>
  );
}
