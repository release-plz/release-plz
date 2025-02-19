// @ts-check
// Note: type annotations allow type checking and IDEs autocompletion

import { themes } from "prism-react-renderer";

const lightCodeTheme = themes.github;
const darkCodeTheme = themes.dracula;

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: "Release-plz",
  tagline: "Publish Rust crates from CI with a Release PR.",
  favicon: "img/favicon.ico",

  // Set the production url of your site here
  url: "https://release-plz.dev",
  // Set the /<baseUrl>/ pathname under which your site is served
  // For GitHub pages deployment, it is often '/<projectName>/'
  baseUrl: "/",

  // GitHub pages deployment config.
  // If you aren't using GitHub pages, you don't need these.
  organizationName: "release-plz", // Usually your GitHub org/user name.
  projectName: "release-plz", // Usually your repo name.
  trailingSlash: false,

  onBrokenLinks: "throw",
  onBrokenMarkdownLinks: "warn",

  // Even if you don't use internalization, you can use this field to set useful
  // metadata like html lang. For example, if your site is Chinese, you may want
  // to replace "en" with "zh-Hans".
  i18n: {
    defaultLocale: "en",
    locales: ["en"],
  },
  scripts: [{ src: "/js/posthog.js" }],
  markdown: {
    mermaid: true,
  },
  themes: [
    '@docusaurus/theme-mermaid',
    [
      require.resolve("@easyops-cn/docusaurus-search-local"),
      /** @type {import("@easyops-cn/docusaurus-search-local").PluginOptions} */
      ({
        // ... Your options.
        // `hashed` is recommended as long-term-cache of index file is possible.
        hashed: true,
        // For Docs using Chinese, The `language` is recommended to set to:
        // ```
        // language: ["en", "zh"],
        // ```
      }),
    ],
  ],

  presets: [
    [
      "classic",
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: require.resolve("./sidebars.js"),
          // Please change this to your repo.
          // Remove this to remove the "edit this page" links.
          editUrl: "https://github.com/release-plz/release-plz/tree/main/website/",
        },
        theme: {
          customCss: require.resolve("./src/css/custom.css"),
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      image: "img/release-plz-social-card.png",
      announcementBar: {
        id: "announcementBar-1", // Increment on change
        content: `⭐️ If you like Release-plz, give it a star on <a target="_blank" rel="noopener noreferrer" href="https://github.com/release-plz/release-plz">GitHub</a> and follow it on <a target="_blank" rel="noopener noreferrer" href="https://bsky.app/profile/release-plz.dev">BlueSky</a>`,
      },
      navbar: {
        title: "Release-plz",
        logo: {
          alt: "Release-plz Logo",
          src: "img/robot_head.jpeg",
        },
        items: [
          {
            type: "docSidebar",
            sidebarId: "tutorialSidebar",
            position: "left",
            label: "Docs",
          },
          {
            type: "docSidebar",
            sidebarId: "tutorialSidebar",
            href: "/pricing",
            position: "left",
            label: "Pricing",
          },
          {
            label: "💖 Sponsor",
            href: "https://github.com/sponsors/MarcoIeni",
            position: "right",
          },
          {
            href: "https://github.com/release-plz/release-plz",
            "aria-label": "GitHub",
            className: "header-github-link",
            position: "right",
          },
        ],
      },
      footer: {
        style: "dark",
        links: [
          {
            title: "Docs",
            items: [
              {
                label: "Tutorial",
                to: "/docs",
              },
            ],
          },
          {
            title: "Social",
            items: [
              {
                label: "BlueSky",
                href: "https://bsky.app/profile/release-plz.dev",
              },
              {
                label: "LinkedIn",
                href: "https://www.linkedin.com/company/release-plz/",
              },
            ],
          },
          {
            title: "Release-plz",
            items: [
              {
                label: "crates.io",
                href: "https://crates.io/crates/release-plz",
              },
              {
                label: "docs.rs",
                href: "https://docs.rs/release_plz_core/",
              },
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} Release-plz. Built with Docusaurus.`,
      },
      prism: {
        theme: lightCodeTheme,
        darkTheme: darkCodeTheme,
        additionalLanguages: ["toml", "json"],
      },
    }),
};

module.exports = config;
