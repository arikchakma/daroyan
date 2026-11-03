import { useQuery } from "@tanstack/react-query";

import { authQueries } from "~/queries/auth";

export function useIsAdmin() {
  const { data: me } = useQuery(authQueries.me());

  return me?.role === "admin";
}
