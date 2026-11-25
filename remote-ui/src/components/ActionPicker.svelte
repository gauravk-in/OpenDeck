<script lang="ts">
	import type { Action, Controller } from "../state.ts";
	import { categories, plugins } from "../state.ts";
	import { imageUrl } from "../api.ts";
	import { createEventDispatcher } from "svelte";

	export let controller: Controller;
	/// When adding to a multi action or toggle action, only actions supported there are offered.
	export let childOf: string | null = null;
	const dispatch = createEventDispatcher<{ pick: Action }>();

	let query = "";
	$: q = query.trim().toLowerCase();
	$: filtered = Object.entries($categories)
		.map(([name, category]) => {
			const actions = category.actions.filter(
				(a) =>
					a.visible_in_action_list &&
					a.controllers.includes(controller) &&
					(!childOf || (a.supported_in_multi_actions && !a.uuid.startsWith("opendeck."))) &&
					(!q || a.name.toLowerCase().includes(q) || name.toLowerCase().includes(q) || a.tooltip?.toLowerCase().includes(q)),
			);
			return { name, icon: category.icon, actions };
		})
		.filter((c) => c.actions.length > 0)
		// Home Assistant first, as this remote is mostly about it, then alphabetical with OpenDeck's built-ins last.
		.sort((a, b) => rank(a.name) - rank(b.name) || a.name.localeCompare(b.name));

	function rank(name: string) {
		if (/home assistant/i.test(name)) return 0;
		if (name == "OpenDeck") return 2;
		return 1;
	}

	function categoryIcon(icon: string | null, actions: Action[]): string | null {
		if (icon) return imageUrl(icon);
		const plugin = $plugins.find((p) => p.id == actions[0]?.plugin);
		return plugin && actions.every((a) => a.plugin == plugin.id) ? imageUrl(plugin.icon) : null;
	}
</script>

<input class="input search" type="search" placeholder="Search actions" bind:value={query} />

{#each filtered as category (category.name)}
	<div class="section-title row">
		{#if categoryIcon(category.icon, category.actions)}
			<img class="cat-icon" src={categoryIcon(category.icon, category.actions)} alt="" />
		{/if}
		{category.name}
	</div>
	<div class="list">
		{#each category.actions as action (action.plugin + action.uuid)}
			<button class="list-item" on:click={() => dispatch("pick", action)}>
				<img class="icon" src={imageUrl(action.icon)} alt="" />
				<span class="text">
					<span class="name">{action.name}</span>
					{#if action.tooltip}<span class="tooltip muted">{action.tooltip}</span>{/if}
				</span>
			</button>
		{/each}
	</div>
{:else}
	<p class="muted empty">No actions {q ? "match your search" : `can go on a ${controller == "Encoder" ? "dial" : "key"}`}.</p>
{/each}

<style>
	.search {
		position: sticky;
		top: 0;
		z-index: 1;
		margin-bottom: 4px;
	}
	.cat-icon {
		width: 18px;
		height: 18px;
		border-radius: 4px;
	}
	.icon {
		width: 40px;
		height: 40px;
		border-radius: 9px;
		border: 1px solid var(--border);
		background: #000;
		flex: none;
		object-fit: cover;
	}
	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.name {
		font-weight: 600;
	}
	.tooltip {
		font-size: 13px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.empty {
		text-align: center;
		margin: 32px 0;
	}
</style>
