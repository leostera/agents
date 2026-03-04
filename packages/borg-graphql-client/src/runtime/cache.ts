export type GraphQLCache = Map<string, unknown>;

export function createGraphQLCache(): GraphQLCache {
  return new Map();
}
