<script lang="ts">
	import type { Slot } from "./state.ts";
	import { serverInfo, setToken } from "./api.ts";
	import { attempt, connected, contextFor, device, instanceAt, layout, profile, refreshAll, refreshDevice, startEvents, stopEvents, toast } from "./state.ts";
	import DeviceView from "./components/DeviceView.svelte";
	import Pair from "./components/Pair.svelte";
	import ProfileSheet from "./components/ProfileSheet.svelte";
	import SettingsSheet from "./components/SettingsSheet.svelte";
	import SlotSheet from "./components/SlotSheet.svelte";
	import { onMount } from "svelte";

	let phase: "loading" | "pair" | "ready" | "offline" = "loading";
	let selected: Slot | null = null;
	let sheet: "profiles" | "settings" | null = null;
	let moving: { slot: Slot; copy: boolean } | null = null;

	async function start() {
		phase = "loading";
		// A pairing link carries the token in the fragment, which never reaches the server or its logs.
		const match = location.hash.match(/token=([A-Za-z0-9]+)/);
		if (match) {
			setToken(match[1]);
			history.replaceState(null, "", location.pathname + location.search);
		}
		try {
			const info = await serverInfo();
			if (!info.authorised) {
				phase = "pair";
				return;
			}
		} catch {
			phase = "offline";
			return;
		}
		phase = "ready";
		startEvents();
		await refreshAll().catch(() => {});
	}

	onMount(() => {
		start();
		return stopEvents;
	});

	function slotLabel(slot: Slot) {
		return slot.controller == "Encoder" ? `dial ${slot.position + 1}` : `key ${slot.position + 1}`;
	}

	async function onSelect(slot: Slot) {
		if (moving) {
			const source = moving;
			moving = null;
			if (source.slot.controller != slot.controller) return;
			if (source.slot.position == slot.position) return;
			await attempt("move_instance", { source: contextFor(source.slot), destination: contextFor(slot), retain: source.copy });
			await refreshDevice();
			return;
		}
		selected = slot;
	}

	$: emptyDeck = $profile && ![...$profile.keys, ...$profile.sliders].some((x) => x);
</script>

{#if phase == "pair"}
	<Pair on:paired={start} />
{:else if phase == "offline"}
	<main class="centre">
		<h1>Can't reach the deck</h1>
		<p class="muted">Check that the deck is on and that this phone is on the same Wi-Fi network.</p>
		<button class="btn primary" on:click={start}>Try again</button>
	</main>
{:else if phase == "loading"}
	<main class="centre"><p class="muted">Connecting…</p></main>
{:else}
	<header>
		<div class="title">
			<span class="dot" class:live={$connected} title={$connected ? "Live" : "Reconnecting"} />
			<span class="device">{$device?.name ?? "No deck connected"}</span>
		</div>
		<button class="btn ghost gear" aria-label="Settings" on:click={() => (sheet = "settings")}>⚙︎</button>
	</header>

	<main>
		{#if $device && $profile}
			<button class="page-picker" on:click={() => (sheet = "profiles")}>
				<span class="muted label">Page</span>
				<span class="page">{$profile.id}</span>
				<span class="chev">▾</span>
			</button>

			{#if moving}
				<div class="banner row">
					<span class="grow"
						>Tap the {moving.slot.controller == "Encoder" ? "dial" : "key"} to {moving.copy ? "copy" : "move"} {slotLabel(moving.slot)} to.</span
					>
					<button class="btn" on:click={() => (moving = null)}>Cancel</button>
				</div>
			{/if}

			<DeviceView device={$device} profile={$profile} layout={$layout} selected={moving?.slot ?? selected} on:select={(e) => onSelect(e.detail)} />

			<p class="muted hint">
				{#if emptyDeck}This page is empty. Tap a key or dial to give it an action.{:else}Tap a key or dial to change it.{/if}
			</p>
		{:else}
			<div class="centre">
				<h1>No deck found</h1>
				<p class="muted">OpenDeck is running but doesn't see a stream controller. Check its USB connection.</p>
			</div>
		{/if}
	</main>

	{#if selected && $profile}
		{#key `${$profile.id}:${selected.controller}:${selected.position}`}
			<SlotSheet
				slot={selected}
				on:close={() => (selected = null)}
				on:move={(e) => {
					moving = e.detail;
					selected = null;
				}}
			/>
		{/key}
	{/if}
	{#if sheet == "profiles"}
		<ProfileSheet on:close={() => (sheet = null)} />
	{:else if sheet == "settings"}
		<SettingsSheet on:close={() => (sheet = null)} on:unpaired={() => ((sheet = null), stopEvents(), (phase = "pair"))} />
	{/if}
{/if}

{#if $toast}
	<div class="toast" class:error={$toast.kind == "error"} role="status">{$toast.text}</div>
{/if}

<style>
	header {
		position: sticky;
		top: 0;
		z-index: 10;
		display: flex;
		align-items: center;
		padding: calc(6px + var(--safe-top)) 8px 6px 16px;
		background: rgba(10, 10, 10, 0.9);
		backdrop-filter: blur(10px);
		border-bottom: 1px solid #1f1f1f;
	}
	.title {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		font-weight: 700;
	}
	.device {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: #f59e0b;
		flex: none;
	}
	.dot.live {
		background: var(--ok);
	}
	.gear {
		font-size: 22px;
		width: 44px;
		padding: 0;
	}
	main {
		padding: 16px 16px calc(24px + var(--safe-bottom));
		max-width: 640px;
		margin: 0 auto;
	}
	.centre {
		text-align: center;
		padding-top: 80px;
	}
	.page-picker {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		min-height: 52px;
		padding: 0 16px;
		margin-bottom: 18px;
		border-radius: var(--radius);
		border: 1px solid var(--border);
		background: var(--surface);
		text-align: left;
	}
	.page-picker .label {
		font-size: 13px;
	}
	.page {
		flex: 1;
		font-weight: 700;
		font-size: 17px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.chev {
		color: var(--muted);
	}
	.banner {
		padding: 10px 10px 10px 14px;
		margin-bottom: 14px;
		border-radius: var(--radius);
		background: rgba(59, 130, 246, 0.15);
		border: 1px solid rgba(59, 130, 246, 0.5);
	}
	.grow {
		flex: 1;
	}
	.hint {
		text-align: center;
		font-size: 14px;
		margin-top: 18px;
	}
	.toast {
		position: fixed;
		left: 50%;
		bottom: calc(20px + var(--safe-bottom));
		transform: translateX(-50%);
		z-index: 100;
		max-width: calc(100% - 32px);
		padding: 12px 16px;
		border-radius: 12px;
		background: #2a2a2a;
		border: 1px solid var(--border);
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
		font-size: 14px;
	}
	.toast.error {
		border-color: var(--danger);
	}
</style>
