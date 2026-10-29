import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'altgo',
  tagline: 'Speech to text, without typing',
  favicon: 'img/favicon.ico',

  future: {
    v4: true,
  },

  url: 'https://ouyangjiahong26.github.io',
  baseUrl: '/altgo/',
  trailingSlash: false,

  organizationName: 'ouyangjiahong26',
  projectName: 'altgo',

  onBrokenLinks: 'throw',

  i18n: {
    defaultLocale: 'en',
    locales: ['en', 'zh-Hans'],
    localeConfigs: {
      en: {
        label: 'English',
        direction: 'ltr',
        htmlLang: 'en',
      },
      'zh-Hans': {
        label: '简体中文',
        direction: 'ltr',
        htmlLang: 'zh-Hans',
      },
    },
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/ouyangjiahong26/altgo/tree/master/docs-site/',
          editLocalizedFiles: true,
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    metadata: [
      {
        name: 'description',
        content:
          'altgo is a desktop speech-to-text tool built with Tauri. It supports Linux and Windows on x86_64 and arm64, transcribes locally with SenseVoice or online, polishes with an optional LLM, and outputs to the clipboard and an overlay.',
      },
    ],
    image: 'img/screenshot-main.png',
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: true,
    },
    navbar: {
      hideOnScroll: false,
      title: 'altgo',
      logo: {
        alt: 'altgo',
        src: 'img/logo.svg',
      },
      style: 'dark',
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docsSidebar',
          position: 'left',
          label: 'Docs',
        },
        {
          href: 'https://github.com/ouyangjiahong26/altgo',
          label: 'GitHub',
          position: 'right',
        },
        {
          type: 'localeDropdown',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {label: 'Quick Start', to: '/docs/quick-start'},
            {label: 'Configuration', to: '/docs/configuration'},
            {label: 'Usage', to: '/docs/usage'},
          ],
        },
        {
          title: 'More',
          items: [
            {label: 'Architecture', to: '/docs/architecture'},
            {label: 'FAQ', to: '/docs/faq'},
            {label: 'Privacy Policy', to: '/docs/privacy'},
          ],
        },
        {
          title: 'Community',
          items: [
            {label: 'GitHub', href: 'https://github.com/ouyangjiahong26/altgo'},
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} ouyangjiahong26. MIT License. Built with Docusaurus.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['toml', 'bash'],
    },
    docs: {
      sidebar: {
        hideable: true,
      },
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
