import { useMutation } from "@tanstack/react-query";
import { useState } from "react";

import { Icon } from "~/components/icons/icon";
import { Button } from "~/components/ui/button";
import { Dialog } from "~/components/ui/dialog";
import { FieldRow } from "~/components/ui/field-row";
import { OptionList } from "~/components/ui/option-list";
import { queryClient } from "~/lib/query-client";
import { toast } from "~/lib/toast";
import { inviteMembersOptions } from "~/queries/members";
import type { MemberRole } from "~/queries/members";

const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

const ROLE_OPTIONS: {
  value: MemberRole;
  title: string;
  description: string;
}[] = [
  {
    value: "admin",
    title: "Admin",
    description:
      "Everything, including servers, checks, crons, alert rules, settings and members.",
  },
  {
    value: "member",
    title: "Member",
    description:
      "Sees everything and handles incidents and mutes. Cannot change monitoring, settings or members.",
  },
];

type InviteMembersDialogProps = {
  takenEmails: string[];
  disabled?: boolean;
};

export function InviteMembersDialog(props: InviteMembersDialogProps) {
  const { takenEmails, disabled } = props;

  const [isOpen, setIsOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const [emails, setEmails] = useState<string[]>([]);
  const [role, setRole] = useState<MemberRole>("member");

  const addEmails = (value: string) => {
    const next = [...emails];
    for (const part of value.split(/[\s,;]+/)) {
      const email = part.trim().toLowerCase();
      if (!email || next.includes(email)) {
        continue;
      }
      if (!EMAIL_PATTERN.test(email)) {
        toast.error(`${email} is not a valid email address`);
        continue;
      }
      if (takenEmails.includes(email)) {
        toast.error(`${email} can already sign in`);
        continue;
      }
      next.push(email);
    }
    setEmails(next);
    setDraft("");
  };

  const invite = useMutation({
    ...inviteMembersOptions(),
    meta: { errorMessage: "The invite could not be added" },
    onSuccess: () => {
      setIsOpen(false);
      toast.success(emails.length === 1 ? "Member invited" : "Members invited");
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["members"] }),
  });

  return (
    <Dialog.Root
      open={isOpen}
      onOpenChange={(open) => {
        setDraft("");
        setEmails([]);
        setRole("member");
        setIsOpen(open);
      }}
    >
      <Dialog.Trigger render={<Button className="w-fit" disabled={disabled} />}>
        Invite Members
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Backdrop />
        <Dialog.Popup className="max-w-120">
          <Dialog.Header>
            <Dialog.Title>Invite Members</Dialog.Title>
          </Dialog.Header>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (draft.trim()) {
                addEmails(draft);
                return;
              }
              invite.mutate({ emails, role });
            }}
          >
            <Dialog.Body className="gap-4">
              <Dialog.Description>
                They sign in with the Google account for this email. No email is
                sent, so let them know.
              </Dialog.Description>
              <FieldRow label="Email Addresses">
                {({ inputId }) => (
                  <>
                    <div className="flex min-h-9 flex-wrap items-center gap-1.5 rounded-lg border border-border-ring bg-surface-default px-2 py-1.5 focus-within:border-border-strong">
                      {emails.map((email) => (
                        <span
                          key={email}
                          className="flex h-6 items-center gap-1 rounded-sm bg-surface-fill pr-1 pl-2 text-body"
                        >
                          {email}
                          <button
                            type="button"
                            aria-label={`Remove ${email}`}
                            onClick={() =>
                              setEmails(emails.filter((item) => item !== email))
                            }
                            className="flex size-4 items-center justify-center rounded-xs text-text-secondary outline-none hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40 [&_svg]:size-3"
                          >
                            <Icon name="x" />
                          </button>
                        </span>
                      ))}
                      <input
                        id={inputId}
                        type="text"
                        inputMode="email"
                        autoComplete="off"
                        value={draft}
                        placeholder={
                          emails.length ? "Add more…" : "name@company.com"
                        }
                        onChange={(event) => {
                          const value = event.target.value;
                          if (/[,;\s]$/.test(value)) {
                            addEmails(value);
                          } else {
                            setDraft(value);
                          }
                        }}
                        onBlur={() => draft.trim() && addEmails(draft)}
                        onKeyDown={(event) => {
                          if (
                            event.key === "Backspace" &&
                            !draft &&
                            emails.length
                          ) {
                            setEmails(emails.slice(0, -1));
                          }
                        }}
                        className="h-6 min-w-32 flex-1 bg-transparent text-body outline-none placeholder:text-text-tertiary"
                      />
                    </div>
                    <p className="text-body text-text-secondary">
                      Separate addresses with commas.
                    </p>
                  </>
                )}
              </FieldRow>
              <div className="flex flex-col gap-1.5">
                <span className="text-body font-medium">Role</span>
                <div className="overflow-hidden rounded-xl border border-border-ring bg-surface-default">
                  <OptionList
                    label="Role"
                    value={role}
                    options={ROLE_OPTIONS}
                    onValueChange={setRole}
                  />
                </div>
              </div>
            </Dialog.Body>
            <Dialog.Footer>
              <Dialog.Close
                render={
                  <Button
                    variant="secondary"
                    className="w-fit"
                    disabled={invite.isPending}
                  />
                }
              >
                Cancel
              </Dialog.Close>
              <Button
                type="submit"
                className="w-fit"
                disabled={
                  invite.isPending || (emails.length === 0 && !draft.trim())
                }
              >
                {invite.isPending && (
                  <Icon name="loader" className="animate-spin" />
                )}
                {emails.length > 1
                  ? `Invite ${emails.length} Members`
                  : "Invite Member"}
              </Button>
            </Dialog.Footer>
          </form>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
