import type { Schemas } from "@daroyan/client";
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";

import { Icon } from "~/components/icons/icon";
import { ConfirmDialog } from "~/components/ui/confirm-dialog";
import { Menu } from "~/components/ui/menu";
import { queryClient } from "~/lib/query-client";
import { toast } from "~/lib/toast";
import { removeMemberOptions, updateMemberOptions } from "~/queries/members";
import type { MemberRole } from "~/queries/members";

type MemberActionsProps = {
  member: Schemas["Member"] & { id: string };
};

export function MemberActions(props: MemberActionsProps) {
  const { member } = props;

  const label = member.user?.name ?? member.email;
  const isInvite = !member.user;
  const [nextRole, setNextRole] = useState<MemberRole | null>(null);
  const [isRemoving, setIsRemoving] = useState(false);

  const changeRole = useMutation({
    ...updateMemberOptions(),
    meta: { errorMessage: "The role could not be changed" },
    onSuccess: () => {
      toast.success("Role changed");
      setNextRole(null);
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["members"] }),
  });

  const remove = useMutation({
    ...removeMemberOptions(),
    meta: {
      errorMessage: isInvite
        ? "The invite could not be revoked"
        : "The member could not be removed",
    },
    onSuccess: () => {
      toast.success(isInvite ? "Invite revoked" : "Member removed");
      setIsRemoving(false);
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["members"] }),
  });

  return (
    <>
      <Menu.Root>
        <Menu.Trigger
          aria-label={`Actions for ${label}`}
          className="inline-flex size-6 items-center justify-center rounded-full opacity-0 outline-none group-hover:opacity-100 hover:bg-surface-fill focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:opacity-100 pointer-coarse:opacity-100"
        >
          <Icon name="more" />
        </Menu.Trigger>
        <Menu.Portal>
          <Menu.Positioner align="end">
            <Menu.Popup>
              <Menu.SubmenuRoot>
                <Menu.SubmenuTrigger>
                  <Icon name="shield" />
                  Change Role
                </Menu.SubmenuTrigger>
                <Menu.Portal>
                  <Menu.Positioner sideOffset={4}>
                    <Menu.Popup>
                      <Menu.RadioGroup
                        value={member.role}
                        onValueChange={(next: MemberRole) =>
                          next !== member.role && setNextRole(next)
                        }
                      >
                        <Menu.RadioItem
                          value="admin"
                          disabled={changeRole.isPending}
                        >
                          Admin
                        </Menu.RadioItem>
                        <Menu.RadioItem
                          value="member"
                          disabled={changeRole.isPending}
                        >
                          Member
                        </Menu.RadioItem>
                      </Menu.RadioGroup>
                    </Menu.Popup>
                  </Menu.Positioner>
                </Menu.Portal>
              </Menu.SubmenuRoot>
              <Menu.Separator />
              <Menu.Item
                className="text-tone-critical-fg"
                onClick={() => setIsRemoving(true)}
              >
                <Icon name={isInvite ? "x" : "user-minus"} />
                {isInvite ? "Revoke Invite" : "Remove Member"}
              </Menu.Item>
            </Menu.Popup>
          </Menu.Positioner>
        </Menu.Portal>
      </Menu.Root>

      <ConfirmDialog
        open={nextRole !== null}
        onOpenChange={(open) => !open && setNextRole(null)}
        tone="default"
        title={nextRole === "admin" ? "Make Admin?" : "Make Member?"}
        description={
          nextRole === "admin"
            ? `${label} can add and change servers, checks, crons, alert rules, settings and members.`
            : `${label} can still see everything and handle incidents, but can no longer change monitoring, settings or members.`
        }
        confirmLabel="Change Role"
        isPending={changeRole.isPending}
        onConfirm={() =>
          nextRole && changeRole.mutate({ memberId: member.id, role: nextRole })
        }
      />
      <ConfirmDialog
        open={isRemoving}
        onOpenChange={setIsRemoving}
        title={isInvite ? "Revoke Invite?" : `Remove ${label}?`}
        description={
          isInvite
            ? `${member.email} can no longer sign in. You can invite them again later.`
            : "They are signed out right away and their MCP tokens stop working."
        }
        details={
          isInvite
            ? undefined
            : [
                "Incidents and updates they wrote keep their name.",
                "They can come back only with a new invite.",
              ]
        }
        confirmLabel={isInvite ? "Revoke Invite" : "Remove Member"}
        isPending={remove.isPending}
        onConfirm={() => remove.mutate({ memberId: member.id })}
      />
    </>
  );
}
