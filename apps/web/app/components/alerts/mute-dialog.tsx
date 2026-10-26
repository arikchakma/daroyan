import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import type { ReactNode } from 'react';

import { getBrowserTimeZone } from '~/components/alerts/quiet-hours-field';
import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { Button } from '~/components/ui/button';
import { DatePicker, fromDayKey, toDayKey } from '~/components/ui/date-picker';
import { Dialog } from '~/components/ui/dialog';
import { Input } from '~/components/ui/input';
import { TimePicker } from '~/components/ui/time-picker';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { createAlertMuteOptions } from '~/queries/alerts';
import { formatMinutesLong } from '~/utils/alerts';
import { cn } from '~/utils/classname';
import { formatClock } from '~/utils/format';

export type MuteLength = '1h' | '4h' | 'tomorrow' | 'resolves' | 'custom';

export type MuteTarget = {
  serverId?: string | null;
  checkId?: string | null;
  cronId?: string | null;
  name: string;
};

const HOUR_MS = 3_600_000;
const SHORT_DAY = new Intl.DateTimeFormat('en-US', { weekday: 'short', month: 'short', day: 'numeric' });
const MONTH_DAY = new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric' });

function getTomorrowMorning() {
  const morning = new Date();
  morning.setDate(morning.getDate() + 1);
  morning.setHours(9, 0, 0, 0);
  return morning;
}

export function getMuteEnd(length: Exclude<MuteLength, 'custom'>) {
  if (length === 'resolves') {
    return null;
  }
  if (length === 'tomorrow') {
    return getTomorrowMorning().toISOString();
  }
  return new Date(Date.now() + (length === '1h' ? 1 : 4) * HOUR_MS).toISOString();
}

export function useCreateMute() {
  return useMutation({
    ...createAlertMuteOptions(),
    meta: { errorMessage: 'The alert could not be muted' },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alert-mutes'] });
      toast.success('Muted');
    },
  });
}

type MuteDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  ruleId: string;
  ruleName: string;
  ruleNoun: string;
  resolveHint: string;
  target?: MuteTarget;
  isFiring: boolean;
};

export function MuteDialog(props: MuteDialogProps) {
  const { open, onOpenChange, ruleId, ruleName, ruleNoun, resolveHint, target, isFiring } = props;

  const [isWholeRule, setIsWholeRule] = useState(!target);
  const [length, setLength] = useState<MuteLength>('1h');
  const [isCustom, setIsCustom] = useState(false);
  const [date, setDate] = useState(() => toDayKey(getTomorrowMorning()));
  const [time, setTime] = useState('09:00');
  const [reason, setReason] = useState('');
  const create = useCreateMute();

  const now = Date.now();
  const customEnd = fromDayKey(date);
  const [hours = 0, minutes = 0] = time.split(':').map(Number);
  customEnd.setHours(hours, minutes, 0, 0);
  const isCustomValid = customEnd.getTime() > now;
  const endsAt = length === 'custom' ? (isCustomValid ? customEnd.toISOString() : undefined) : getMuteEnd(length);
  const tomorrow = getTomorrowMorning().getTime();
  const confirmLabel =
    length === 'custom'
      ? `Mute Until ${MONTH_DAY.format(customEnd)}`
      : length === 'resolves'
        ? 'Mute Until Resolved'
        : length === 'tomorrow'
          ? 'Mute Until Tomorrow'
          : `Mute for ${length === '1h' ? '1 Hour' : '4 Hours'}`;

  const showCustom = () => {
    setIsCustom(true);
    setLength('custom');
  };

  const submit = () => {
    if (endsAt === undefined) {
      return;
    }
    create.mutate(
      {
        rule_id: ruleId,
        server_id: isWholeRule ? null : target?.serverId,
        check_id: isWholeRule ? null : target?.checkId,
        cron_id: isWholeRule ? null : target?.cronId,
        ends_at: endsAt,
        reason: reason.trim() || null,
        time_zone: getBrowserTimeZone(),
      },
      { onSuccess: () => onOpenChange(false) }
    );
  };

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Backdrop />
        <Dialog.Popup>
          <Dialog.Header>
            <Dialog.Title>Mute {ruleName}</Dialog.Title>
          </Dialog.Header>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              submit();
            }}
          >
            <Dialog.Body className="max-h-[calc(100vh-180px)] overflow-y-auto">
              <Dialog.Description>Alerts keep firing and show in Daroyan. Only the messages stop.</Dialog.Description>
              {target && (
                <ChoiceGroup label="Mute">
                  <Choice
                    icon={target.checkId ? 'activity' : target.cronId ? 'clock' : 'server'}
                    title={`Only ${target.name}`}
                    detail={`The alert firing on this ${target.checkId ? 'check' : target.cronId ? 'cron' : 'server'} now`}
                    isSelected={!isWholeRule}
                    onSelect={() => setIsWholeRule(false)}
                  />
                  <Choice
                    icon="bell-off"
                    title="The whole rule"
                    detail={`Every ${ruleNoun} ${ruleName} covers, including new alerts`}
                    isSelected={isWholeRule}
                    onSelect={() => setIsWholeRule(true)}
                  />
                </ChoiceGroup>
              )}
              <ChoiceGroup label="For">
                <Choice
                  icon="clock"
                  title="1 hour"
                  detail={`Until ${formatClock(now + HOUR_MS)}`}
                  isSelected={length === '1h'}
                  onSelect={() => setLength('1h')}
                />
                <Choice
                  icon="clock"
                  title="4 hours"
                  detail={`Until ${formatClock(now + 4 * HOUR_MS)}`}
                  isSelected={length === '4h'}
                  onSelect={() => setLength('4h')}
                />
                {!isCustom && (
                  <Choice
                    icon="clock"
                    title="Until tomorrow, 09:00"
                    detail={`Overnight, about ${formatMinutesLong(now, tomorrow)}`}
                    isSelected={length === 'tomorrow'}
                    onSelect={() => setLength('tomorrow')}
                  />
                )}
                {isFiring && (
                  <Choice
                    icon="clock"
                    title="Until it resolves"
                    detail={resolveHint}
                    isSelected={length === 'resolves'}
                    onSelect={() => setLength('resolves')}
                  />
                )}
                {isCustom && (
                  <Choice
                    icon="clock"
                    title="Custom"
                    detail={
                      isCustomValid
                        ? `Until ${SHORT_DAY.format(customEnd)}, ${time} · about ${formatMinutesLong(now, customEnd.getTime())}`
                        : 'Pick a time in the future'
                    }
                    isSelected={length === 'custom'}
                    onSelect={() => setLength('custom')}
                  />
                )}
              </ChoiceGroup>
              {length === 'custom' && (
                <div className="flex flex-col gap-1.5">
                  <span className="text-body">Until</span>
                  <div className="flex gap-3">
                    <div className="min-w-0 flex-1">
                      <DatePicker value={date} onValueChange={setDate} min={toDayKey(new Date())} />
                    </div>
                    <div className="w-32 shrink-0">
                      <TimePicker value={time} onValueChange={setTime} />
                    </div>
                  </div>
                </div>
              )}
              <label className="flex flex-col gap-1.5 text-body">
                Reason
                <Input value={reason} onValueChange={setReason} maxLength={200} placeholder="Deploying the new worker" />
                <span className="flex items-start gap-1.5 text-caption text-text-tertiary">
                  <Icon name="info" className="mt-0.5 size-3.5 shrink-0" />
                  Optional. Shown on the alert and in the mute message, so the team knows why it is quiet.
                </span>
              </label>
            </Dialog.Body>
            <Dialog.Footer>
              {!isCustom && (
                <Button type="button" variant="plain" className="mr-auto w-fit" onClick={showCustom}>
                  Custom Time…
                </Button>
              )}
              <Dialog.Close render={<Button variant="secondary" className="w-fit" disabled={create.isPending} />}>
                Cancel
              </Dialog.Close>
              <Button type="submit" className="w-fit" disabled={create.isPending || endsAt === undefined}>
                {create.isPending ? <Icon name="loader" className="animate-spin" /> : <Icon name="bell-off" />}
                {confirmLabel}
              </Button>
            </Dialog.Footer>
          </form>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

type ChoiceGroupProps = {
  label: string;
  children: ReactNode;
};

function ChoiceGroup(props: ChoiceGroupProps) {
  const { label, children } = props;

  return (
    <div role="radiogroup" aria-label={label} className="flex flex-col gap-2">
      <span className="text-body">{label}</span>
      <div className="flex flex-col overflow-hidden rounded-xl border border-border-ring [&>*+*]:border-t [&>*+*]:border-border-row">
        {children}
      </div>
    </div>
  );
}

type ChoiceProps = {
  icon: IconName;
  title: string;
  detail: string;
  isSelected: boolean;
  onSelect: () => void;
};

function Choice(props: ChoiceProps) {
  const { icon, title, detail, isSelected, onSelect } = props;

  return (
    <button
      type="button"
      role="radio"
      aria-checked={isSelected}
      onClick={onSelect}
      className={cn(
        'flex items-center gap-3 px-3.5 py-3 text-left outline-none hover:bg-surface-subtle focus-visible:ring-2 focus-visible:ring-text-link/40 focus-visible:ring-inset',
        isSelected && 'bg-surface-subtle'
      )}
    >
      <Icon name={icon} className="size-5 shrink-0" />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="text-body">{title}</span>
        <span className="text-caption text-text-secondary">{detail}</span>
      </span>
      {isSelected && <Icon name="check" className="shrink-0" />}
    </button>
  );
}
