import type { Schemas } from "@daroyan/client";
import { createColumnHelper } from "@tanstack/react-table";
import { useMemo } from "react";

import { MemberActions } from "~/components/members/member-actions";
import { Avatar } from "~/components/ui/avatar";
import { Badge } from "~/components/ui/badge";
import { useDataTable } from "~/hooks/use-data-table";
import type { DataTableFeatures } from "~/lib/table";
import { formatDate } from "~/utils/format";

type MemberRow = { id: string; member: Schemas["Member"] };

const helper = createColumnHelper<DataTableFeatures, MemberRow>();

type UseMembersTableOptions = {
  members: Schemas["Member"][];
  currentEmail?: string;
  canManage: boolean;
};

export function useMembersTable(options: UseMembersTableOptions) {
  const { members, currentEmail, canManage } = options;

  const columns = useMemo(
    () =>
      helper.columns([
        helper.display({
          id: "member",
          header: "Member",
          meta: { skeleton: "avatar-lines-lg" },
          cell: ({ row }) => {
            const { user, email, invited_at: invitedAt } = row.original.member;

            return (
              <div className="flex items-center gap-2.5 py-2.5 text-body">
                <Avatar
                  id={user?.id ?? email}
                  src={user?.avatar_url}
                  shape="circle"
                  className="size-7"
                />
                <div className="flex min-w-0 flex-col">
                  <span className="truncate">{user ? user.name : email}</span>
                  <span className="truncate text-text-secondary">
                    {user
                      ? email
                      : `Invited ${invitedAt ? formatDate(invitedAt) : ""}`}
                  </span>
                </div>
              </div>
            );
          },
        }),
        helper.display({
          id: "role",
          header: "Role",
          meta: { width: 112, skeleton: "badge", hideBelow: "md" },
          cell: ({ row }) => (
            <Badge tone="neutral">
              {row.original.member.is_locked
                ? "Owner"
                : row.original.member.role === "admin"
                  ? "Admin"
                  : "Member"}
            </Badge>
          ),
        }),
        helper.display({
          id: "status",
          header: "Status",
          meta: { width: 112, align: "right", skeleton: "badge" },
          cell: ({ row }) =>
            row.original.member.user ? (
              <Badge tone="success">Active</Badge>
            ) : (
              <Badge tone="attention">Invited</Badge>
            ),
        }),
        helper.display({
          id: "actions",
          header: () => <span className="sr-only">Actions</span>,
          meta: { width: 36, align: "right", skeleton: "none" },
          cell: ({ row }) => {
            const { member } = row.original;
            const { id, is_locked: isLocked, email } = member;

            if (!canManage || isLocked || !id || email === currentEmail) {
              return null;
            }

            return <MemberActions member={{ ...member, id }} />;
          },
        }),
      ]),
    [currentEmail, canManage],
  );

  const data = useMemo(
    () => members.map((member) => ({ id: member.email, member })),
    [members],
  );

  return useDataTable({ columns, data });
}
