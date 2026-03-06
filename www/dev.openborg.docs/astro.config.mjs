// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	integrations: [
		starlight({
			title: 'Standalone Borg Docs',
			description: 'Operator documentation for running and configuring standalone borg.',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/leostera/borg' }],
			sidebar: [
				{
					label: 'Start Here',
					items: [
						{ label: 'Overview', slug: '' },
						{ label: 'Get Started', slug: 'get-started' },
						{ label: 'Changelog', slug: 'changelog' },
					],
				},
				{
					label: 'Get Started',
					autogenerate: { directory: 'get-started' },
				},
				{
					label: 'Learn',
					autogenerate: { directory: 'learn' },
				},
				{
					label: 'Providers',
					autogenerate: { directory: 'providers' },
				},
				{
					label: 'Models',
					autogenerate: { directory: 'models' },
				},
				{
					label: 'Apps',
					autogenerate: { directory: 'apps' },
				},
				{
					label: 'Integrations',
					autogenerate: { directory: 'integrations' },
				},
				{
					label: 'Changelog',
					items: [{ label: 'Changelog', slug: 'changelog' }],
				},
			],
		}),
	],
});
