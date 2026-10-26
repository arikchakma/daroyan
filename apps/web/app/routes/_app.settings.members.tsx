import { useQuery } from "@tanstack/react-query";

import { InviteMembersDialog } from "~/components/members/invite-members-dialog";
import {
  SettingsCard,
  SettingsPage,
} from "~/components/settings/settings-page";
import { DataTable } from "~/components/ui/data-table";
import { useMembersTable } from "~/hooks/use-members-table";
import { meOptions } from "~/queries/auth";
import { listMembersOptions } from "~/queries/members";

export function meta() {
  return [{ title: "Members · Daroyan" }];
}

export default function MembersPage() {
  const { data: me } = useQuery(meOptions());
  const { data: members, isLoading } = useQuery(listMembersOptions());
  const canManage = me?.role === "admin";

  const table = useMembersTable({
    members: members ?? [],
    currentEmail: me?.email,
    canManage,
  });

  return (
    <SettingsPage
      title="Members"
      icon="users"
      description="People who can sign in with Google. Owners come from ALLOWED_EMAILS and always stay admins."
      actions={
        canManage && (
          <InviteMembersDialog
            takenEmails={(members ?? []).map((member) => member.email)}
            disabled={!members}
          />
        )
      }
    >
      <SettingsCard>
        <DataTable
          table={table}
          label="Members"
          variant="card"
          size="lg"
          isLoading={isLoading}
        />
      </SettingsCard>
    </SettingsPage>
  );
}
