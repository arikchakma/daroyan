import { mutationOptions, queryOptions } from "@tanstack/react-query";

import { apiClient } from "~/lib/api-client";
import { request } from "~/lib/http";

export function listMcpTokensOptions() {
  return queryOptions({
    queryKey: ["mcp-tokens"],
    queryFn: () => request(apiClient.GET("/v1/mcp-tokens")),
  });
}

export function createMcpTokenOptions() {
  return mutationOptions({
    mutationFn: (body: { name: string }) =>
      request(apiClient.POST("/v1/mcp-tokens", { body })),
  });
}

export function deleteMcpTokenOptions() {
  return mutationOptions({
    mutationFn: (variables: { tokenId: string }) =>
      request(
        apiClient.DELETE("/v1/mcp-tokens/{token_id}", {
          params: { path: { token_id: variables.tokenId } },
        }),
      ),
  });
}
