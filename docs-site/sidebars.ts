import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    {
      type: 'doc',
      id: 'intro',
      label: '简介',
    },
    {
      type: 'category',
      label: '开始使用',
      items: ['quick-start', 'configuration', 'usage'],
      collapsed: false,
    },
    {
      type: 'category',
      label: '深入',
      items: ['architecture', 'faq'],
      collapsed: false,
    },
    {
      type: 'doc',
      id: 'privacy',
      label: 'Privacy Policy / 隐私政策',
    },
  ],
};

export default sidebars;
