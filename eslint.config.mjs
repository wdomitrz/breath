const commonRules = {
  curly: "error",
  eqeqeq: ["error", "always"],
  "no-implicit-globals": "error",
  "no-redeclare": "error",
  "no-shadow": "error",
  "no-undef": "error",
  "no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
  "no-var": "error",
  "object-shorthand": "error",
  "prefer-const": "error",
  "prefer-template": "error",
};

export default [
  {
    files: ["app.js"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "script",
      globals: {
        console: "readonly",
        document: "readonly",
        navigator: "readonly",
      },
    },
    rules: commonRules,
  },
  {
    files: ["sw.js"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "script",
      globals: {
        caches: "readonly",
        fetch: "readonly",
        Promise: "readonly",
        self: "readonly",
        URL: "readonly",
      },
    },
    rules: commonRules,
  },
];
