import type { Schemas } from '@daroyan/client';
import { useMutation } from '@tanstack/react-query';
import type { JSONContent } from '@tiptap/core';
import { useEffect, useId, useRef, useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { RichText } from '~/components/rich-text/rich-text';
import { RichTextEditor } from '~/components/rich-text/rich-text-editor';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { Checkbox } from '~/components/ui/checkbox';
import { Menu } from '~/components/ui/menu';
import { Switch } from '~/components/ui/switch';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import {
  publishPostmortemOptions,
  savePostmortemOptions,
  unpublishPostmortemOptions,
} from '~/queries/incidents';
import { cn } from '~/utils/classname';
import { formatMoment } from '~/utils/incidents';
import type { Incident } from '~/utils/incidents';
import { hasText, toDocument } from '~/utils/rich-text';

const SAVE_DELAY_MS = 800;

type FollowUp = Schemas['IncidentFollowUp'];

function heading(text: string): JSONContent {
  return { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text }] };
}

const TEMPLATE: JSONContent = {
  type: 'doc',
  content: [
    heading('What happened'),
    { type: 'paragraph' },
    heading('Impact'),
    { type: 'paragraph' },
    heading('Root cause'),
    { type: 'paragraph' },
  ],
};

type PostmortemCardProps = {
  incident: Incident;
  users: { id: string; name: string }[];
  shownOn: string;
  channelNames: string;
};

export function PostmortemCard(props: PostmortemCardProps) {
  const { incident, users, shownOn, channelNames } = props;

  const saved = toDocument(incident.postmortem);
  const isPublished = !!incident.postmortem_published_at;
  const [isEditing, setIsEditing] = useState(!isPublished);
  const [content, setContent] = useState<JSONContent | null>(null);
  const [notify, setNotify] = useState(true);
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved'>('idle');
  const notifyId = useId();
  const pending = useRef<JSONContent | null>(null);
  const latestFollowUps = useRef(incident.follow_ups);

  useEffect(() => {
    latestFollowUps.current = incident.follow_ups;
  }, [incident.follow_ups]);
  const hasPages = incident.status_pages.some((page) => page.is_shown);
  const hasChannels = incident.channel_ids.length > 0;

  const save = useMutation({
    ...savePostmortemOptions(),
    meta: { errorMessage: 'The postmortem could not be saved' },
    onMutate: () => setSaveState('saving'),
    onSuccess: (next) => {
      queryClient.setQueryData(['incidents', incident.id], next);
      setSaveState('saved');
    },
    onError: () => setSaveState('idle'),
  });

  const publish = useMutation({
    ...publishPostmortemOptions(),
    meta: { errorMessage: 'The postmortem could not be published' },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] });
      toast.success(isPublished ? 'Postmortem updated' : 'Postmortem published');
      setIsEditing(false);
    },
  });

  const unpublish = useMutation({
    ...unpublishPostmortemOptions(),
    meta: { errorMessage: 'The postmortem could not be unpublished' },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] });
      toast.success('Postmortem unpublished');
      setIsEditing(true);
    },
  });

  const persist = (postmortem: JSONContent | null, followUps: FollowUp[]) =>
    save.mutateAsync({
      incidentId: incident.id,
      body: { postmortem: postmortem && hasText(postmortem) ? postmortem : null, follow_ups: followUps },
    });

  useEffect(() => {
    if (!content) {
      return;
    }
    pending.current = content;
    const timer = window.setTimeout(() => {
      pending.current = null;
      void persist(content, latestFollowUps.current);
    }, SAVE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [content]);

  const current = content ?? saved;
  const setFollowUps = (followUps: FollowUp[]) => {
    latestFollowUps.current = followUps;
    void persist(pending.current ?? current, followUps);
  };

  const doPublish = async () => {
    if (!current || !hasText(current)) {
      toast.error('Write the postmortem before you publish it');
      return;
    }
    if (pending.current || content) {
      await persist(current, incident.follow_ups);
      pending.current = null;
    }
    publish.mutate({ incidentId: incident.id, body: { notify: notify && hasChannels } });
  };

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <h2 className="text-title-section font-medium">Postmortem</h2>
        {isPublished ? (
          <Badge tone="success">
            <Icon name="check" />
            Published
          </Badge>
        ) : (
          <Badge>Draft</Badge>
        )}
        <span className="flex-1" />
        <span className="flex items-center gap-1 text-small text-text-secondary">
          {isPublished && !isEditing ? (
            <>
              <Icon name="check" className="size-3.5" />
              Published {formatMoment(incident.postmortem_published_at!).toLowerCase()}
            </>
          ) : saveState === 'saving' ? (
            'Saving…'
          ) : (
            <>
              <Icon name="check" className="size-3.5" />
              Saved as you type
            </>
          )}
        </span>
      </div>

      <div className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
        {isEditing ? (
          <RichTextEditor
            hasHeadings
            size="prose"
            label="Postmortem"
            placeholder="Write what happened, the impact and the root cause."
            content={saved ?? TEMPLATE}
            onChange={setContent}
            contentClassName="min-h-40 px-5 py-4"
            toolbarClassName="border-b-0 px-3"
          />
        ) : (
          <RichText content={saved} size="prose" className="px-5 py-4" />
        )}

        <FollowUps followUps={incident.follow_ups} users={users} onChange={setFollowUps} canEdit={isEditing} />

        {isEditing ? (
          <div className="flex flex-wrap items-center gap-4 px-4 py-3 max-sm:flex-col max-sm:items-stretch">
            <div className="flex flex-1 flex-wrap items-center gap-4">
              <p className="text-small text-text-secondary">
                {hasPages ? `Visitors see it ${shownOn} once you publish.` : 'No status page shows this incident.'}
              </p>
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
            {isPublished && (
              <Button variant="secondary" className="w-fit" onClick={() => setIsEditing(false)}>
                Cancel
              </Button>
            )}
            <Button className="w-fit max-sm:w-full" disabled={publish.isPending} onClick={() => void doPublish()}>
              {isPublished ? 'Save and Publish' : 'Publish Postmortem'}
            </Button>
          </div>
        ) : (
          <div className="flex flex-wrap items-center gap-3 px-4 py-3">
            <p className="min-w-0 flex-1 text-small text-text-secondary">
              Visitors see it on the incident page. Edits go live when you save.
            </p>
            <Button
              variant="secondary"
              className="w-fit"
              disabled={unpublish.isPending}
              onClick={() => unpublish.mutate({ incidentId: incident.id })}
            >
              Unpublish
            </Button>
            <Button variant="secondary" className="w-fit" onClick={() => setIsEditing(true)}>
              <Icon name="pencil" />
              Edit Postmortem
            </Button>
          </div>
        )}
      </div>
    </section>
  );
}

type FollowUpsProps = {
  followUps: FollowUp[];
  users: { id: string; name: string }[];
  canEdit: boolean;
  onChange: (followUps: FollowUp[]) => void;
};

function FollowUps(props: FollowUpsProps) {
  const { followUps, users, canEdit, onChange } = props;

  const [draft, setDraft] = useState<string | null>(null);
  const names = new Map(users.map((user) => [user.id, user.name]));

  const add = () => {
    const body = draft?.trim();
    if (!body) {
      setDraft(null);
      return;
    }
    onChange([...followUps, { id: '', body, owner_id: null, is_done: false }]);
    setDraft('');
  };

  return (
    <div className="flex flex-col gap-1 px-4 pt-3.5 pb-2">
      <h3 className="pb-1 text-body font-medium">Follow-Ups</h3>
      {followUps.length === 0 && draft === null && (
        <p className="py-1 text-body text-text-secondary">No follow-ups yet.</p>
      )}
      <ul className="flex flex-col">
        {followUps.map((followUp, index) => (
          <li key={followUp.id || index} className="flex min-h-8.5 items-center gap-2.5">
            <Checkbox
              aria-label={followUp.body}
              checked={followUp.is_done}
              onCheckedChange={(isDone) =>
                onChange(followUps.map((other, otherIndex) => (otherIndex === index ? { ...other, is_done: isDone } : other)))
              }
            />
            <span
              className={cn(
                'min-w-0 flex-1 text-body',
                followUp.is_done ? 'text-text-tertiary line-through' : 'text-text-secondary'
              )}
            >
              {followUp.body}
            </span>
            <Menu.Root>
              <Menu.Trigger
                aria-label={`Owner of ${followUp.body}`}
                className="flex h-5.5 shrink-0 items-center rounded-xs bg-surface-fill px-1.75 text-caption font-normal text-text-secondary outline-none hover:bg-surface-fill-strong focus-visible:ring-2 focus-visible:ring-text-link/40"
              >
                {(followUp.owner_id && names.get(followUp.owner_id)) || 'No owner'}
              </Menu.Trigger>
              <Menu.Portal>
                <Menu.Positioner align="end">
                  <Menu.Popup>
                    <Menu.RadioGroup
                      value={followUp.owner_id ?? ''}
                      onValueChange={(ownerId: string) =>
                        onChange(
                          followUps.map((other, otherIndex) =>
                            otherIndex === index ? { ...other, owner_id: ownerId || null } : other
                          )
                        )
                      }
                    >
                      <Menu.RadioItem value="">No owner</Menu.RadioItem>
                      {users.map((user) => (
                        <Menu.RadioItem key={user.id} value={user.id}>
                          {user.name}
                        </Menu.RadioItem>
                      ))}
                    </Menu.RadioGroup>
                    {canEdit && (
                      <>
                        <Menu.Separator />
                        <Menu.Item
                          className="text-tone-critical-fg"
                          onClick={() => onChange(followUps.filter((_, otherIndex) => otherIndex !== index))}
                        >
                          <Icon name="trash" />
                          Remove
                        </Menu.Item>
                      </>
                    )}
                  </Menu.Popup>
                </Menu.Positioner>
              </Menu.Portal>
            </Menu.Root>
          </li>
        ))}
      </ul>
      {canEdit &&
        (draft === null ? (
          <Button variant="plain" size="xs" className="-ml-2.5" onClick={() => setDraft('')}>
            <Icon name="plus" />
            Add Follow-Up
          </Button>
        ) : (
          <form
            className="flex min-h-8.5 items-center gap-2.5"
            onSubmit={(event) => {
              event.preventDefault();
              add();
            }}
          >
            <Checkbox aria-hidden="true" checked={false} disabled />
            <input
              autoFocus
              aria-label="New follow-up"
              value={draft}
              maxLength={500}
              placeholder="Add a 30 minute timeout to the backup cron"
              onChange={(event) => setDraft(event.target.value)}
              onBlur={add}
              onKeyDown={(event) => event.key === 'Escape' && setDraft(null)}
              className="min-w-0 flex-1 bg-transparent text-body text-text-primary outline-none placeholder:text-text-tertiary"
            />
          </form>
        ))}
    </div>
  );
}
