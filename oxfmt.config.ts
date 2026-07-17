import { defineConfig } from 'oxfmt';

export default defineConfig({
  ignorePatterns: ['vendor/webrtc-audio-processing/**', 'vendor/webrtc-audio-processing-sys/**'],
  singleQuote: true,
  jsxSingleQuote: true,
  sortImports: true,
  sortTailwindcss: true,
});
