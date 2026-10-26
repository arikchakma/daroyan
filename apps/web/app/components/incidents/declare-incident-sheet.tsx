import { useMutation, useQuery } from '@tanstack/react-query';
import type { JSONContent } from '@tiptap/core';
import { useState } from 'react';
import { useNavigate } from 'react-router';

import {
  AffectedComponentsCard,
  FormCard,
  IncidentSheet,
  NotifyColumn,
  OptionSelect,
  RelatedField,
  Segmented,
  SheetRow,
  SheetSection,
  useIncidentDirectory,
} from '~/components/incidents/incident-form';
import type { AffectedComponent, RelatedItem } from '~/components/incidents/incident-form';
import { Icon } from '~/components/icons/icon';
import { RichTextEditor } from '~/components/rich-text/rich-text-editor';
import { Input } from '~/components/ui/input';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { meOptions } from '~/queries/auth';
import { createIncidentOptions } from '~/queries/incidents';
import { formatClock } from '~/utils/format';
import { SEVERITY_META } from '~/utils/incidents';
import { hasText } from '~/utils/rich-text';

const SEVERITY_OPTIONS = (['critical', 'major', 'minor'] as const).map((value) => ({
  value,
  label: SEVERITY_META[value]!.label,
}));

const STATUS_OPTIONS = [
  { value: 'investigating', label: 'Investigating' },
  { value: 'identified', label: 'Identified' },
  { value: 'monitoring', label: 'Monitoring' },
] as const;

const STARTED_OPTIONS = [0, 5, 15, 30, 60, 120] as const;

export type DeclareDefaults = {
  title?: string;
  severity?: 'minor' | 'major' | 'critical';
  components?: AffectedComponent[];
  related?: RelatedItem[];
  alertId?: string;
  source?: string;
};

type DeclareIncidentSheetProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaults?: DeclareDefaults;
};

export function DeclareIncidentSheet(props: DeclareIncidentSheetProps) {
  const { open, onOpenChange, defaults } = props;

  if (!open) {
    return null;
  }

  return <DeclareIncidentForm onOpenChange={onOpenChange} defaults={defaults} />;
}

type DeclareIncidentFormProps = {
  onOpenChange: (open: boolean) => void;
  defaults?: DeclareDefaults;
};

function DeclareIncidentForm(props: DeclareIncidentFormProps) {
  const { onOpenChange, defaults } = props;

  const navigate = useNavigate();
  const directory = useIncidentDirectory();
  const { data: me } = useQuery(meOptions());

  const [title, setTitle] = useState(defaults?.title ?? '');
  const [severity, setSeverity] = useState<'minor' | 'major' | 'critical'>(defaults?.severity ?? 'major');
  const [pickedLeadId, setLeadId] = useState<string | null | undefined>(undefined);
  const leadId = pickedLeadId === undefined ? (me?.id ?? null) : pickedLeadId;
  const [startedMinutes, setStartedMinutes] = useState(0);
  const [related, setRelated] = useState<RelatedItem[]>(defaults?.related ?? []);
  const [components, setComponents] = useState<AffectedComponent[]>(defaults?.components ?? []);
  const [status, setStatus] = useState<(typeof STATUS_OPTIONS)[number]['value']>('investigating');
  const [message, setMessage] = useState<JSONContent | null>(null);
  const [hiddenPageIds, setHiddenPageIds] = useState<string[]>([]);
  const [pickedChannelIds, setChannelIds] = useState<string[] | null>(null);
  const channelIds = pickedChannelIds ?? directory.channels.map((channel) => channel.id);

  const create = useMutation({
    ...createIncidentOptions(),
    meta: { errorMessage: 'The incident could not be declared' },
    onSuccess: (incident) => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] });
      toast.success('Incident declared');
      onOpenChange(false);
      navigate(`/incidents/${incident.id}`);
    },
  });

  const now = Date.now();
  const startedItems = STARTED_OPTIONS.map((minutes) => ({
    value: String(minutes),
    label:
      minutes === 0
        ? `Now · ${formatClock(now)}`
        : `${minutes < 60 ? `${minutes} minutes` : `${minutes / 60} ${minutes === 60 ? 'hour' : 'hours'}`} ago · ${formatClock(now - minutes * 60_000)}`,
  }));
  const userItems = directory.users.map((user) => ({ value: user.id, label: user.name }));

  const submit = () => {
    if (!title.trim()) {
      toast.error('Add a title');
      return;
    }
    if (!message || !hasText(message)) {
      toast.error('Write the first update');
      return;
    }
    create.mutate({
      kind: 'incident',
      title: title.trim(),
      severity,
      lead_id: leadId,
      started_at: new Date(Date.now() - startedMinutes * 60_000).toISOString(),
      components,
      related,
      responders: [],
      channel_ids: channelIds,
      hidden_page_ids: hiddenPageIds,
      alert_id: defaults?.alertId,
      update: { status, body: message },
    });
  };

  return (
    <IncidentSheet
      open
      onOpenChange={onOpenChange}
      icon="flag"
      title="Declare Incident"
      submitLabel="Declare Incident"
      isPending={create.isPending}
      onSubmit={submit}
      aside={
        <NotifyColumn
          directory={directory}
          components={components}
          hiddenPageIds={hiddenPageIds}
          onHiddenPageIdsChange={setHiddenPageIds}
          channelIds={channelIds}
          onChannelIdsChange={setChannelIds}
        />
      }
    >
      {defaults?.source && (
        <p className="flex items-center gap-2 rounded-xl bg-surface-fill px-3 py-2 text-body text-text-secondary">
          <Icon name="bell" className="shrink-0" />
          Filled in from {defaults.source}. Check it before you declare.
        </p>
      )}
      <SheetSection title="Incident" description="Say what is broken. You can change this later.">
        <FormCard>
          <SheetRow label="Title" htmlFor="incident-title">
            <Input
              id="incident-title"
              autoFocus
              value={title}
              placeholder="Postgres is not accepting connections"
              onChange={(event) => setTitle(event.target.value)}
              className="flex-1"
            />
          </SheetRow>
          <SheetRow label="Severity">
            <Segmented label="Severity" value={severity} options={SEVERITY_OPTIONS} onChange={setSeverity} />
          </SheetRow>
          <SheetRow label="Lead">
            <OptionSelect
              label="Lead"
              value={leadId}
              placeholder="No lead"
              items={userItems}
              onChange={setLeadId}
            />
          </SheetRow>
          <SheetRow label="Started At">
            <OptionSelect
              label="Started at"
              value={String(startedMinutes)}
              items={startedItems}
              onChange={(value) => setStartedMinutes(Number(value ?? 0))}
            />
          </SheetRow>
          <SheetRow label="Related">
            <RelatedField directory={directory} related={related} onChange={setRelated} />
          </SheetRow>
        </FormCard>
      </SheetSection>

      <SheetSection
        title="Affected Components"
        description="Pick uptime checks and crons. The incident shows on every status page that includes them."
      >
        <AffectedComponentsCard directory={directory} components={components} onChange={setComponents} />
      </SheetSection>

      <SheetSection title="First Update" description="The first entry in the timeline.">
        <FormCard>
          <SheetRow label="Status">
            <Segmented label="Status" value={status} options={STATUS_OPTIONS} onChange={setStatus} />
          </SheetRow>
          <SheetRow label="Message" isTall>
            <RichTextEditor
              label="First update"
              placeholder="Say what you see and what you are doing about it."
              content={message}
              onChange={setMessage}
              className="flex-1 overflow-hidden rounded-xl border border-border-ring bg-surface-default"
            />
          </SheetRow>
        </FormCard>
      </SheetSection>
    </IncidentSheet>
  );
}
