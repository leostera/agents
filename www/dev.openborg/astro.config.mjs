// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	integrations: [
		starlight({
			title: 'openborg',
			description: 'Marketing site plus platform developer docs for openborg.',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/leostera/borg' }],
			sidebar: [
				{
					label: 'Start Here',
					items: [
						{ label: 'Marketing Home', slug: '' },
						{ label: 'Get Started', link: 'https://docs.openborg.dev/' },
						{ label: 'For Developers', slug: 'platform' },
					],
				},
				{
					label: 'Marketing',
					items: [
						{ label: 'Comparison Hub', slug: 'compare' },
						{ label: 'Works Well With', slug: 'works-well-with' },
						{ label: 'Content Map', slug: 'playbooks/content-map' },
						{ label: 'Product Hunt Signals', slug: 'playbooks/producthunt-signals' },
					],
				},
				{
					label: 'Platform Dev: APIs',
					autogenerate: { directory: 'platform/api' },
				},
				{
					label: 'Platform Dev: Extending Borg',
					autogenerate: { directory: 'platform/apps' },
				},
				{
					label: 'Platform Dev: Knowledge Base',
					autogenerate: { directory: 'platform/knowledge-base' },
				},
			],
		}),
	],
});
