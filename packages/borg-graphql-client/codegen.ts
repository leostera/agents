import type { CodegenConfig } from "@graphql-codegen/cli";

const scalars = {
  DateTime: "string",
  JsonValue: "unknown",
  Uri: "string",
};

const config: CodegenConfig = {
  overwrite: true,
  schema: "../../crates/borg-gql/schema.graphql",
  documents: ["../../apps/**/*.{graphql,gql}", "../../packages/**/*.{graphql,gql}"],
  generates: {
    "./src/generated/types.ts": {
      plugins: ["typescript"],
      config: {
        scalars,
      },
    },
    "./src/generated/operations.ts": {
      plugins: ["typescript", "typescript-operations", "typed-document-node"],
      config: {
        scalars,
      },
    },
  },
};

export default config;
