// @ts-check
// Note: type annotations allow type checking and IDEs autocompletion

import { themes } from "prism-react-renderer";

const lightCodeTheme = themes.github;
const darkCodeTheme = themes.dracula;

const TwitterSvg =
  '<svg style="fill: #1DA1F2; vertical-align: middle; margin-left: 3px;" width="16" height="16" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><path d="M459.37 151.716c.325 4.548.325 9.097.325 13.645 0 138.72-105.583 298.558-298.558 298.558-59.452 0-114.68-17.219-161.137-47.106 8.447.974 16.568 1.299 25.34 1.299 49.055 0 94.213-16.568 130.274-44.832-46.132-.975-84.792-31.188-98.112-72.772 6.498.974 12.995 1.624 19.818 1.624 9.421 0 18.843-1.3 27.614-3.573-48.081-9.747-84.143-51.98-84.143-102.985v-1.299c13.969 7.797 30.214 12.67 47.431 13.319-28.264-18.843-46.781-51.005-46.781-87.391 0-19.492 5.197-37.36 14.294-52.954 51.655 63.675 129.3 105.258 216.365 109.807-1.624-7.797-2.599-15.918-2.599-24.04 0-57.828 46.782-104.934 104.934-104.934 30.213 0 57.502 12.67 76.67 33.137 23.715-4.548 46.456-13.32 66.599-25.34-7.798 24.366-24.366 44.833-46.132 57.827 21.117-2.273 41.584-8.122 60.426-16.243-14.292 20.791-32.161 39.308-52.628 54.253z"></path></svg>';

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
        content: `⭐️ If you like Release-plz, give it a star on <a target="_blank" rel="noopener noreferrer" href="https://github.com/release-plz/release-plz">GitHub</a> and follow the author on <a target="_blank" rel="noopener noreferrer" href="https://twitter.com/MarcoIeni">Twitter ${TwitterSvg}</a>`,
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
            title: "Author",
            items: [
              {
                label: "Twitter",
                href: "https://twitter.com/MarcoIeni",
              },
              {
                label: "LinkedIn",
                href: "https://www.linkedin.com/in/marcoieni/",
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
