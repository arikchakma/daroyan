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
  SheetRow,
  SheetSection,
  ToggleRow,
  useIncidentDirectory,
} from '~/components/incidents/incident-form';
import type { AffectedComponent } from '~/components/incidents/incident-form';
import { RichTextEditor } from '~/components/rich-text/rich-text-editor';
import { DatePicker, fromDayKey, toDayKey } from '~/components/ui/date-picker';
import { Input } from '~/components/ui/input';
import { TimePicker } from '~/components/ui/time-picker';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { meOptions } from '~/queries/auth';
import { createIncidentOptions } from '~/queries/incidents';
import { formatClock } from '~/utils/format';
import { hasText } from '~/utils/rich-text';

const HOUR_MS = 3_600_000;

type DateTime = { day: string; time: string };

function toDateTime(value: number): DateTime {
  const date = new Date(value);
  return {
    day: toDayKey(date),
    time: `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`,
  };
}

function fromDateTime(value: DateTime) {
  const date = fromDayKey(value.day);
  const [hours = 0, minutes = 0] = value.time.split(':').map(Number);
  date.setHours(hours, minutes, 0, 0);
  return date.getTime();
}

function getNextHour() {
  const date = new Date(Date.now() + HOUR_MS);
  date.setMinutes(0, 0, 0);
  return date.getTime();
}

type DateTimeFieldProps = {
  label: string;
  value: DateTime;
  onChange: (value: DateTime) => void;
};

function DateTimeField(props: DateTimeFieldProps) {
  const { label, value, onChange } = props;

  return (
    <div role="group" aria-label={label} className="flex min-w-0 flex-1 gap-2">
      <div className="min-w-0 flex-1">
        <DatePicker value={value.day} onValueChange={(day) => onChange({ ...value, day })} min={toDayKey(new Date())} />
      </div>
      <div className="w-32 shrink-0">
        <TimePicker value={value.time} onValueChange={(time) => onChange({ ...value, time })} />
      </div>
    </div>
  );
}

type ScheduleMaintenanceSheetProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function ScheduleMaintenanceSheet(props: ScheduleMaintenanceSheetProps) {
  const { open, onOpenChange } = props;

  if (!open) {
    return null;
  }

  return <ScheduleMaintenanceForm onOpenChange={onOpenChange} />;
}

function ScheduleMaintenanceForm(props: { onOpenChange: (open: boolean) => void }) {
  const { onOpenChange } = props;

  const navigate = useNavigate();
  const directory = useIncidentDirectory();
  const { data: me } = useQuery(meOptions());

  const [title, setTitle] = useState('');
  const [starts, setStarts] = useState(() => toDateTime(getNextHour()));
  const [ends, setEnds] = useState(() => toDateTime(getNextHour() + HOUR_MS));
  const [pickedLeadId, setLeadId] = useState<string | null | undefined>(undefined);
  const leadId = pickedLeadId === undefined ? (me?.id ?? null) : pickedLeadId;
  const [components, setComponents] = useState<AffectedComponent[]>([]);
  const [message, setMessage] = useState<JSONContent | null>(null);
  const [hiddenPageIds, setHiddenPageIds] = useState<string[]>([]);
  const [pickedChannelIds, setChannelIds] = useState<string[] | null>(null);
  const channelIds = pickedChannelIds ?? directory.channels.map((channel) => channel.id);
  const [autoStart, setAutoStart] = useState(true);
  const [autoFinish, setAutoFinish] = useState(true);
  const [remindBefore, setRemindBefore] = useState(true);

  const create = useMutation({
    ...createIncidentOptions(),
    meta: { errorMessage: 'The maintenance could not be scheduled' },
    onSuccess: (incident) => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] });
      toast.success('Maintenance scheduled');
      onOpenChange(false);
      navigate(`/incidents/${incident.id}`);
    },
  });

  const startsAt = fromDateTime(starts);
  const endsAt = fromDateTime(ends);
  const userItems = directory.users.map((user) => ({ value: user.id, label: user.name }));

  const submit = () => {
    if (!title.trim()) {
      toast.error('Add a title');
      return;
    }
    if (endsAt <= startsAt) {
      toast.error('The maintenance has to end after it starts');
      return;
    }
    if (!message || !hasText(message)) {
      toast.error('Write a message for your status pages');
      return;
    }
    create.mutate({
      kind: 'maintenance',
      title: title.trim(),
      lead_id: leadId,
      starts_at: new Date(startsAt).toISOString(),
      ends_at: new Date(endsAt).toISOString(),
      auto_start: autoStart,
      auto_finish: autoFinish,
      remind_before: remindBefore,
      components,
      related: [],
      responders: [],
      channel_ids: channelIds,
      hidden_page_ids: hiddenPageIds,
      update: { body: message },
    });
  };

  return (
    <IncidentSheet
      open
      onOpenChange={onOpenChange}
      icon="calendar"
      title="Schedule Maintenance"
      submitLabel="Schedule"
      isPending={create.isPending}
      onSubmit={submit}
      aside={
        <>
          <NotifyColumn
            directory={directory}
            components={components}
            hiddenPageIds={hiddenPageIds}
            onHiddenPageIdsChange={setHiddenPageIds}
            channelIds={channelIds}
            onChannelIdsChange={setChannelIds}
          />
          <h2 className="mt-3 text-title-section font-medium">Automation</h2>
          <FormCard>
            <ToggleRow
              title="Start on its own"
              description={`Components go Under Maintenance at ${formatClock(startsAt)}`}
              checked={autoStart}
              onCheckedChange={setAutoStart}
            />
            <ToggleRow
              title="Finish on its own"
              description={`Components go back to their monitor status at ${formatClock(endsAt)}`}
              checked={autoFinish}
              onCheckedChange={setAutoFinish}
            />
            <ToggleRow
              title="Remind an hour before"
              description={`Posts to the channels above at ${formatClock(startsAt - HOUR_MS)}`}
              checked={remindBefore}
              onCheckedChange={setRemindBefore}
            />
          </FormCard>
        </>
      }
    >
      <SheetSection title="Maintenance" description="Plan downtime ahead so visitors know before it happens.">
        <FormCard>
          <SheetRow label="Title" htmlFor="maintenance-title">
            <Input
              id="maintenance-title"
              autoFocus
              value={title}
              placeholder="Postgres upgrade to 17"
              onChange={(event) => setTitle(event.target.value)}
              className="flex-1"
            />
          </SheetRow>
          <SheetRow label="Starts">
            <DateTimeField label="Starts" value={starts} onChange={setStarts} />
          </SheetRow>
          <SheetRow label="Ends">
            <DateTimeField label="Ends" value={ends} onChange={setEnds} />
          </SheetRow>
          <SheetRow label="Lead">
            <OptionSelect label="Lead" value={leadId} placeholder="No lead" items={userItems} onChange={setLeadId} />
          </SheetRow>
        </FormCard>
      </SheetSection>

      <SheetSection
        title="Affected Components"
        description="They show Under Maintenance during the window. Uptime and cron alerts for them are muted while it runs."
      >
        <AffectedComponentsCard directory={directory} components={components} onChange={setComponents} isMaintenance />
      </SheetSection>

      <SheetSection title="Message" description="Shown on status pages from now until the window ends.">
        <RichTextEditor
          label="Maintenance message"
          placeholder="Say what changes and what visitors might notice."
          content={message}
          onChange={setMessage}
          className="overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card"
          contentClassName="min-h-24 px-4"
        />
      </SheetSection>
    </IncidentSheet>
  );
}
