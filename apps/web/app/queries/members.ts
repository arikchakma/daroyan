import { mutationOptions, queryOptions } from "@tanstack/react-query";

import { apiClient } from "~/lib/api-client";
import { request } from "~/lib/http";

export type MemberRole = "admin" | "member";

export function listMembersOptions() {
  return queryOptions({
    queryKey: ["members"],
    queryFn: () => request(apiClient.GET("/v1/members")),
  });
}

export function inviteMembersOptions() {
  return mutationOptions({
    mutationFn: (variables: { emails: string[]; role: MemberRole }) =>
      Promise.all(
        variables.emails.map((email) =>
          request(
            apiClient.POST("/v1/members", {
              body: { email, role: variables.role },
            }),
          ),
        ),
      ),
  });
}

export function updateMemberOptions() {
  return mutationOptions({
    mutationFn: (variables: { memberId: string; role: MemberRole }) =>
      request(
        apiClient.PATCH("/v1/members/{member_id}", {
          params: { path: { member_id: variables.memberId } },
          body: { role: variables.role },
        }),
      ),
  });
}

export function removeMemberOptions() {
  return mutationOptions({
    mutationFn: (variables: { memberId: string }) =>
      request(
        apiClient.DELETE("/v1/members/{member_id}", {
          params: { path: { member_id: variables.memberId } },
        }),
      ),
  });
}
