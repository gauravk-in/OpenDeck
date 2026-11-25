<script lang="ts">
	import type { DeviceInfo, LayoutPosition, Profile, Slot } from "../state.ts";
	import { images, instanceAt, pressed, replicated, slotKey } from "../state.ts";
	import SlotTile from "./SlotTile.svelte";
	import { createEventDispatcher } from "svelte";

	export let device: DeviceInfo;
	export let profile: Profile;
	export let layout: Record<string, LayoutPosition> | null;
	export let selected: Slot | null = null;

	const dispatch = createEventDispatcher<{ select: Slot }>();

	type Placed = Slot & { x: number; y: number; scale: number; key: string };

	function place(device: DeviceInfo, layout: Record<string, LayoutPosition> | null): Placed[] {
		const keys = device.rows * device.columns;
		const slots: Placed[] = [];
		for (let i = 0; i < keys; i++) {
			const key = slotKey("Keypad", i);
			const pos = layout?.[key] ?? { x: i % device.columns, y: Math.floor(i / device.columns) };
			slots.push({ controller: "Keypad", position: i, key, x: pos.x, y: pos.y, scale: pos.scale ?? 1 });
		}
		// Without a custom layout, dials go in a row under the keys, centred.
		const offset = Math.max(0, (device.columns - device.encoders) / 2);
		for (let i = 0; i < device.encoders; i++) {
			const key = slotKey("Encoder", i);
			const pos = layout?.[key] ?? { x: offset + i, y: device.rows };
			slots.push({ controller: "Encoder", position: i, key, x: pos.x, y: pos.y, scale: pos.scale ?? 1 });
		}
		return slots;
	}

	$: placed = place(device, layout);
	$: width = Math.max(1, ...placed.map((s) => s.x + s.scale));
	$: height = Math.max(1, ...placed.map((s) => s.y + s.scale));
</script>

<div class="deck" style:aspect-ratio="{width} / {height}" style:--w={width}>
	{#each placed as slot (slot.key)}
		<div
			class="cell"
			style:left="{(slot.x / width) * 100}%"
			style:top="{(slot.y / height) * 100}%"
			style:width="{(slot.scale / width) * 100}%"
			style:height="{(slot.scale / height) * 100}%"
		>
			<SlotTile
				controller={slot.controller}
				instance={instanceAt(profile, slot)}
				rendered={slot.controller == "Keypad" ? $images[slot.key] : undefined}
				replicated={$replicated.includes(slot.key)}
				pressed={!!$pressed[slot.key]}
				selected={selected?.controller == slot.controller && selected.position == slot.position}
				label={slot.controller == "Encoder" ? `Dial ${slot.position + 1}` : `Key ${slot.position + 1}`}
				on:click={() => dispatch("select", { controller: slot.controller, position: slot.position })}
			/>
		</div>
	{/each}
</div>

<style>
	.deck {
		position: relative;
		width: 100%;
		max-width: min(560px, calc(var(--w) * 150px));
		max-height: 62dvh;
		margin: 0 auto;
	}
	.cell {
		position: absolute;
		padding: clamp(4px, 1.6vw, 9px);
	}
</style>
