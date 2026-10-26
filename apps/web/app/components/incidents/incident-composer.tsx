import { useMutation } from '@tanstack/react-query';
import type { JSONContent } from '@tiptap/core';
import { useId, useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { RichTextEditor } from '~/components/rich-text/rich-text-editor';
import { Button } from '~/components/ui/button';
import { Switch } from '~/components/ui/switch';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { postIncidentUpdateOptions } from '~/queries/incidents';
import { cn } from '~/utils/classname';
import { getStatusMeta, statusesOf } from '~/utils/incidents';
import type { Incident } from '~/utils/incidents';
import { hasText } from '~/utils/rich-text';

const CHIP_CLASS_NAMES: Record<string, string> = {
  critical: 'aria-checked:bg-tone-critical-bg aria-checked:text-tone-critical-fg',
  warning: 'aria-checked:bg-tone-warning-bg aria-checked:text-tone-warning-fg',
  info: 'aria-checked:bg-tone-info-bg aria-checked:text-tone-info-fg',
  success: 'aria-checked:bg-tone-success-bg aria-checked:text-tone-success-fg',
  magic: 'aria-checked:bg-tone-magic-bg aria-checked:text-tone-magic-fg',
};

const NOTE = 'note';

type IncidentComposerProps = {
  incident: Incident;
  shownOn: string;
  channelNames: string;
};

export function IncidentComposer(props: IncidentComposerProps) {
  const { incident, shownOn, channelNames } = props;

  const statuses = statusesOf(incident.kind);
  const [mode, setMode] = useState<string>(incident.status);
  const [body, setBody] = useState<JSONContent | null>(null);
  const [version, setVersion] = useState(0);
  const [isPublic, setIsPublic] = useState(true);
  const [notify, setNotify] = useState(true);
  const publicId = useId();
  const notifyId = useId();

  const isNote = mode === NOTE;
  const hasPages = incident.status_pages.some((page) => page.is_shown);
  const hasChannels = incident.channel_ids.length > 0;

  const post = useMutation({
    ...postIncidentUpdateOptions(),
    meta: { errorMessage: isNote ? 'The note could not be added' : 'The update could not be posted' },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] });
      toast.success(isNote ? 'Note added' : 'Update posted');
      setBody(null);
      setVersion((current) => current + 1);
    },
  });

  const submit = () => {
    if (!body || !hasText(body)) {
      toast.error(isNote ? 'Write the note first' : 'Write the update first');
      return;
    }
    post.mutate({
      incidentId: incident.id,
      body: isNote
        ? { kind: 'note', body }
        : { kind: 'update', status: mode, body, is_public: isPublic && hasPages, notify: notify && hasChannels },
    });
  };

  return (
    <div className="flex flex-col overflow-hidden rounded-[28px] border border-border-ring bg-surface-default shadow-card">
      <div className="flex flex-wrap items-center gap-1.5 p-3">
        <span id={`${publicId}-status`} className="text-body font-medium">
          Status
        </span>
        <div role="radiogroup" aria-labelledby={`${publicId}-status`} className="flex flex-wrap items-center gap-1.5">
          {statuses.map((status) => {
            const meta = getStatusMeta(status);
            return (
              <button
                key={status}
                type="button"
                role="radio"
                aria-checked={mode === status}
                onClick={() => setMode(status)}
                className={cn(
                  'flex h-5.5 items-center gap-1 rounded-sm bg-tone-neutral-bg pr-2 pl-1.5 text-caption font-medium text-tone-neutral-fg outline-none focus-visible:ring-2 focus-visible:ring-text-link/40 [&_svg]:size-3.5',
                  CHIP_CLASS_NAMES[meta.tone]
                )}
              >
                {mode === status && <Icon name={meta.icon} />}
                {meta.label}
              </button>
            );
          })}
          <span aria-hidden="true" className="h-4 w-px bg-border-ring" />
          <button
            type="button"
            role="radio"
            aria-checked={isNote}
            onClick={() => setMode(NOTE)}
            className="flex h-5.5 items-center gap-1 rounded-sm bg-tone-neutral-bg pr-2 pl-1.5 text-caption font-medium text-tone-neutral-fg outline-none focus-visible:ring-2 focus-visible:ring-text-link/40 aria-checked:bg-highlight-note aria-checked:text-tone-attention-fg [&_svg]:size-3.5"
          >
            {isNote && <Icon name="lock" />}
            Note
          </button>
        </div>
      </div>
      <RichTextEditor
        key={version}
        label={isNote ? 'Internal note' : 'Update'}
        placeholder={isNote ? 'Only your team sees notes…' : 'Write an update…'}
        content={body}
        onChange={setBody}
        className="border-t border-border-ring"
        toolbarClassName="border-border-ring"
      />
      <div className="flex flex-wrap items-center gap-4 border-t border-border-ring p-3 max-sm:flex-col max-sm:items-stretch">
        {isNote ? (
          <p className="flex-1 text-small text-text-secondary">Only your team sees notes. They never reach status pages or channels.</p>
        ) : (
          <div className="flex flex-1 flex-wrap items-center gap-4">
            <label className="flex items-center gap-2 text-body">
              <Switch.Root
                aria-labelledby={publicId}
                checked={isPublic && hasPages}
                disabled={!hasPages}
                onCheckedChange={setIsPublic}
              >
                <Switch.Thumb />
              </Switch.Root>
              <span id={publicId}>{hasPages ? `Show ${shownOn}` : 'No status page shows this'}</span>
            </label>
            <label className="flex items-center gap-2 text-body">
              <Switch.Root
                aria-labelledby={notifyId}
                checked={notify && hasChannels}
                disabled={!hasChannels}
                onCheckedChange={setNotify}
              >
                <Switch.Thumb />
              </Switch.Root>
              <span id={notifyId}>{hasChannels ? `Notify ${channelNames}` : 'No channels to notify'}</span>
            </label>
          </div>
        )}
        <Button className="w-fit max-sm:w-full" disabled={post.isPending} onClick={submit}>
          {isNote ? 'Add Note' : 'Post Update'}
        </Button>
      </div>
    </div>
  );
}
