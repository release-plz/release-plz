/**
 * Creating a sidebar enables you to:
 - create an ordered group of docs
 - render a sidebar for each doc of that group
 - provide next/previous navigation

 The sidebars can be generated from the filesystem, or explicitly defined here.

 Create as many sidebars as you want.
 */

// @ts-check

/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  tutorialSidebar: [
    "intro",
    {
      type: "category",
      label: "CLI Usage",
      collapsed: true,
      link: { type: "doc", id: "usage/index" },
      items: [
        "usage/installation",
        "usage/update",
        "usage/release-pr",
        "usage/release",
        "usage/init",
        "usage/set-version",
        "usage/shell-completion",
        "usage/generate-schema",
      ],
    },
    {
      type: "category",
      label: "GitHub Action",
      collapsed: true,
      link: { type: "doc", id: "github/index" },
      items: [
        "github/quickstart",
        "github/input",
        "github/output",
        "github/token",
        "github/update",
        "github/advanced",
        "github/security",
        "github/persist-credentials",
      ],
    },
    "gitlab",
    "config",
    {
      type: "category",
      label: "Changelog",
      collapsed: true,
      link: { type: "doc", id: "changelog/index" },
      items: ["changelog/format", "changelog/examples", "changelog/tips-and-tricks"],
    },
    "semver-check",
    "faq",
    "why",
    "troubleshooting",
    {
      type: "category",
      label: "Extra",
      collapsed: true,
      link: { type: "doc", id: "extra/index" },
      items: [
        "extra/releasing-binaries",
        "extra/single-changelog",
        "extra/single-tag",
        "extra/yanked-packages",
      ],
    },
    "media",
    "release-plz-in-the-wild",
  ],
};

module.exports = sidebars;
