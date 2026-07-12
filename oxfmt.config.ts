import { defineConfig } from 'oxfmt';

export default defineConfig({
  ignorePatterns: ['vendor/**'],
  singleQuote: true,
  jsxSingleQuote: true,
  sortImports: true,
  sortTailwindcss: true,
});
