import { defineConfig } from "vite-plus";
import { externalDistPlugin } from "./src/plugins/dist.ts";

export default defineConfig({
  staged: {
    "*.{js,jsx,ts,tsx,mjs,cjs}": "vp lint",
    "*": "vp fmt --no-error-on-unmatched-pattern",
  },
  fmt: {},
  lint: { options: { typeAware: true, typeCheck: true } },
  plugins: [externalDistPlugin()],
});
