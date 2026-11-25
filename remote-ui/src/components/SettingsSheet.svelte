<script lang="ts">
	import { setToken } from "../api.ts";
	import { attempt, connected, devices, deviceId, plugins, selectDevice } from "../state.ts";
	import Sheet from "./Sheet.svelte";
	import { createEventDispatcher, onMount } from "svelte";

	const dispatch = createEventDispatcher<{ close: void; unpaired: void }>();
	let settings: any = null;
	let version = "";

	onMount(async () => {
		settings = await attempt("get_settings");
		const info = await attempt<{ version: string }>("get_remote_info");
		version = info?.version ?? "";
	});

	let saveTimer: ReturnType<typeof setTimeout> | null = null;
	function save() {
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = setTimeout(() => attempt("set_settings", { settings }), 250);
	}

	function unpair() {
		setToken(null);
		dispatch("unpaired");
	}
</script>

<Sheet title="Settings" on:close={() => dispatch("close")}>
	{#if Object.keys($devices).length > 1}
		<div class="section-title">Device</div>
		<div class="list">
			{#each Object.values($devices) as d (d.id)}
				<button class="list-item" on:click={() => selectDevice(d.id)}>
					<span class="grow">{d.name}</span>
					{#if d.id == $deviceId}<span class="accent">●</span>{/if}
				</button>
			{/each}
		</div>
	{/if}

	{#if settings}
		<div class="section-title">Deck</div>
		<div class="list pad">
			<label class="field">
				<span>Brightness: {settings.brightness}%</span>
				<input type="range" min="0" max="100" bind:value={settings.brightness} on:input={save} />
			</label>
			<label class="field">
				<span>Turn off after</span>
				<select class="input" bind:value={settings.sleep_timeout_minutes} on:change={save}>
					<option value={0}>Never</option>
					<option value={1}>1 minute</option>
					<option value={2}>2 minutes</option>
					<option value={5}>5 minutes</option>
					<option value={10}>10 minutes</option>
					<option value={30}>30 minutes</option>
					<option value={60}>1 hour</option>
				</select>
			</label>
		</div>
	{/if}

	<div class="section-title">Plugins</div>
	<div class="list">
		{#each $plugins as plugin (plugin.id)}
			<div class="list-item">
				<span class="grow">
					<span class="name">{plugin.name}</span>
					<span class="muted small">{plugin.version}{plugin.registered ? "" : " · not running"}</span>
				</span>
				<button class="btn ghost" on:click={() => attempt("reload_plugin", { id: plugin.id })}>Reload</button>
			</div>
		{/each}
	</div>

	<div class="section-title">This phone</div>
	<div class="list">
		<div class="list-item">
			<span class="grow">Connection</span>
			<span class:ok={$connected} class="muted">{$connected ? "Live" : "Reconnecting…"}</span>
		</div>
		{#if version}
			<div class="list-item"><span class="grow">OpenDeck</span><span class="muted">{version}</span></div>
		{/if}
		<button class="list-item danger" on:click={unpair}>Unpair this phone</button>
	</div>
</Sheet>

<style>
	.grow {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.pad {
		padding: 14px 14px 0;
	}
	.name {
		font-weight: 600;
	}
	.small {
		font-size: 13px;
	}
	.accent {
		color: var(--accent);
	}
	.ok {
		color: var(--ok);
	}
	.danger {
		color: var(--danger);
	}
	input[type="range"] {
		width: 100%;
		accent-color: var(--accent);
		height: 32px;
	}
</style>
