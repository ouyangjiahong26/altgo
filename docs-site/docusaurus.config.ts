import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'altgo',
  tagline: '无需打字，言出法随',
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
    defaultLocale: 'zh-Hans',
    locales: ['zh-Hans'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/ouyangjiahong26/altgo/tree/master/docs-site/',
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
          'altgo：Linux 语音转文字桌面工具（Tauri）。SenseVoice 本地转写，可选 OpenAI 兼容 LLM 润色；剪贴板与悬浮窗输出。支持 x86_64 与 aarch64。',
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
          label: '文档',
        },
        {
          href: 'https://github.com/ouyangjiahong26/altgo',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: '文档',
          items: [
            {label: '快速开始', to: '/docs/quick-start'},
            {label: '配置指南', to: '/docs/configuration'},
            {label: '使用说明', to: '/docs/usage'},
          ],
        },
        {
          title: '更多',
          items: [
            {label: '架构设计', to: '/docs/architecture'},
            {label: '常见问题', to: '/docs/faq'},
          ],
        },
        {
          title: '社区',
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
