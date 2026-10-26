import { useQuery } from "@tanstack/react-query";

import { meOptions } from "~/queries/auth";

export function useIsAdmin() {
  const { data: me } = useQuery(meOptions());

  return me?.role === "admin";
}
