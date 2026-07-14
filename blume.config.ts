import { defineConfig } from "blume";

export default defineConfig({
  title: "Templiqx",
  description:
    "Provider-neutral AI interaction contract compiler — portable templiqx/v1alpha1 contracts, deterministic compilation, and CRM3 conformance.",

  github: {
    owner: "RyanLisse",
    repo: "templiqx",
    branch: "main",
    dir: "",
  },

  content: {
    root: "docs",
    exclude: [
      "**/_*",
      "**/.*",
      "**/README.md",
      "**/plans/**",
      "**/brainstorms/**",
      "**/specs/**",
      "**/* 2.*",
      "**/wiki/.last-update.json",
    ],
  },

  navigation: {
    tabs: [
      { label: "Handbook", path: "/", icon: "book-open" },
      { label: "Code docs", path: "/wiki/quickstart", icon: "code" },
    ],
  },

  lastModified: true,

  ai: {
    llmsTxt: true,
  },

  seo: {
    og: { enabled: true },
    sitemap: true,
    robots: true,
    structuredData: true,
  },

  deployment: {
    output: "static",
    site: "https://ryanlisse.github.io/templiqx",
    base: "/templiqx",
  },
});
