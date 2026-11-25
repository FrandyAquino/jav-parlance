import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist"] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.strict],
    files: ["**/*.ts"],
    languageOptions: { globals: { ...globals.browser, ...globals.serviceworker, chrome: "readonly" } },
    rules: {
      "@typescript-eslint/consistent-type-imports": "error",
    },
  },
);
