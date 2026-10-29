import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    {
      type: 'doc',
      id: 'intro',
      label: 'Introduction',
    },
    {
      type: 'category',
      label: 'Getting Started',
      items: ['quick-start', 'configuration', 'usage'],
      collapsed: false,
    },
    {
      type: 'category',
      label: 'Internals',
      items: ['architecture', 'faq'],
      collapsed: false,
    },
    {
      type: 'doc',
      id: 'privacy',
      label: 'Privacy Policy',
    },
  ],
};

export default sidebars;
