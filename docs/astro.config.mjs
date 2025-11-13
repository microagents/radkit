// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://radkit.rs',
	integrations: [
		starlight({
			title: 'Radkit - Rust Agent Development Kit',
			logo: {
				src: './src/assets/logo.svg',
			},
			customCss: [
				'./src/styles/custom.css',
			],
			social: [
				{ icon: 'github', label: 'GitHub', href: 'https://github.com/microagents/radkit' }
			],
			editLink: {
				baseUrl: 'https://github.com/Microagents/Radkit/edit/main/docs/',
			},
			sidebar: [
				{
					label: 'Getting Started',
					slug: 'getting-started',
				},
				{
					label: 'Core Concepts',
					items: [
						{ label: 'Introduction', slug: 'core-concepts' },
						{ label: 'Threads', slug: 'core-concepts/threads' },
						{ label: 'Content', slug: 'core-concepts/content' },
						{ label: 'Events', slug: 'core-concepts/events' },
						{ label: 'LLM Providers', slug: 'core-concepts/llm-providers' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Structured Outputs', slug: 'guides/structured-outputs' },
						{ label: 'Tool Execution', slug: 'guides/tool-execution' },
						{ label: 'Stateful Tools', slug: 'guides/stateful-tools' },
					],
				},
				{
					label: 'A2A Agents',
					items: [
						{ label: 'Introduction to A2A', slug: 'a2a/introduction' },
						{ label: 'Skills', slug: 'a2a/skills' },
						{ label: 'Multi-turn Conversations', slug: 'a2a/multi-turn-conversations' },
						{ label: 'Progress Updates', slug: 'a2a/progress-updates' },
						{ label: 'Composing Agents', slug: 'a2a/composing-agents' },
						{ label: 'A2A Compliance', slug: 'a2a/a2a-compliance' },
					],
				},
				{
					label: 'Architecture',
					items: [
						{ label: 'Overview', slug: 'architecture' },
						{ label: 'Runtime', slug: 'architecture/runtime' },
					],
				},
			],
		}),
	],
});
