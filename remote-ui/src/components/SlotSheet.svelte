<script lang="ts">
	import type { Action, ActionInstance, Slot } from "../state.ts";
	import { attempt, contextFor, device, instanceAt, notify, profile, refreshDevice, replicated, slotKey } from "../state.ts";
	import { imageUrl } from "../api.ts";
	import ActionPicker from "./ActionPicker.svelte";
	import PropertyInspector from "./PropertyInspector.svelte";
	import Sheet from "./Sheet.svelte";
	import { createEventDispatcher } from "svelte";

	export let slot: Slot;
	const dispatch = createEventDispatcher<{ close: void; move: { slot: Slot; copy: boolean } }>();

	$: label = slot.controller == "Encoder" ? `Dial ${slot.position + 1}` : `Key ${slot.position + 1}`;
	$: instance = instanceAt($profile, slot);
	$: isParent = !!instance?.children;
	$: isReplicated = $replicated.includes(slotKey(slot.controller, slot.position));

	/// "picker" chooses the slot's action, "child" adds an action to a multi or toggle action.
	/// An empty slot always shows the picker.
	let mode: "edit" | "picker" | "child" = "edit";

	// Title editing, saved shortly after typing stops.
	let title = "";
	let showTitle = true;
	let editingFor: string | null = null;
	$: if (instance && editingFor != `${instance.context}:${instance.current_state}`) {
		editingFor = `${instance.context}:${instance.current_state}`;
		const state = instance.states[instance.current_state];
		title = state?.text ?? "";
		showTitle = state?.show ?? true;
	}
	let saveTimer: ReturnType<typeof setTimeout> | null = null;
	function queueTitleSave() {
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = setTimeout(saveTitle, 500);
	}
	async function saveTitle() {
		saveTimer = null;
		if (!instance) return;
		const index = instance.current_state;
		const state = instance.states[index];
		if (!state || (state.text == title && state.show == showTitle)) return;
		await attempt("set_state", { context: instance.context, index, state: { ...state, text: title, show: showTitle } });
		refreshDevice();
	}

	async function pick(action: Action) {
		// "Change" replaces the action: OpenDeck only places an action on an empty slot (or inside a multi/toggle action).
		if (mode == "picker" && instance) await attempt("remove_instance", { context: instance.context });
		const result = await attempt<ActionInstance | null>("create_instance", { action, context: contextFor(slot) });
		if (result === null) notify(`${action.name} can't go on this ${slot.controller == "Encoder" ? "dial" : "key"}.`, "error");
		await refreshDevice();
		mode = "edit";
	}

	async function remove(target: ActionInstance) {
		await attempt("remove_instance", { context: target.context });
		await refreshDevice();
		if (target == instance) dispatch("close");
	}

	async function press() {
		await attempt("trigger_virtual_press", { context: contextFor(slot) });
	}

	async function toggleReplicated() {
		await attempt("set_slot_replicated", { context: contextFor(slot), replicated: !isReplicated });
		await refreshDevice();
	}

	function close() {
		if (saveTimer) {
			clearTimeout(saveTimer);
			saveTitle();
		}
		dispatch("close");
	}
</script>

{#if mode != "edit" || !instance}
	<Sheet
		title={mode == "child" ? `Add to ${instance?.action.name}` : `Choose an action for ${label}`}
		back={!!instance}
		on:back={() => (mode = "edit")}
		on:close={close}
	>
		<ActionPicker controller={slot.controller} childOf={mode == "child" ? (instance?.action.uuid ?? null) : null} on:pick={(e) => pick(e.detail)} />
	</Sheet>
{:else}
	<Sheet title={label} on:close={close}>
		<div class="summary row">
			<img class="icon" src={imageUrl(instance.action.icon)} alt="" />
			<div class="grow">
				<div class="name">{instance.action.name}</div>
				<div class="muted small">{instance.action.plugin}</div>
			</div>
			<button class="btn" on:click={() => (mode = "picker")}>Change</button>
		</div>

		{#if slot.controller == "Keypad"}
			<label class="field">
				<span>Title</span>
				<textarea class="input" rows="2" bind:value={title} on:input={queueTitleSave} on:blur={saveTitle} placeholder="No title" />
			</label>
			<label class="row toggle">
				<span class="grow">Show title on key</span>
				<input class="switch" type="checkbox" bind:checked={showTitle} on:change={saveTitle} />
			</label>
		{/if}

		{#if instance.states.length > 1}
			<p class="muted small">
				This action has {instance.states.length} states. The title above is for state {instance.current_state + 1}, which the key shows now.
			</p>
		{/if}

		{#if isParent}
			<div class="section-title">Actions in this {instance.action.uuid == "opendeck.toggleaction" ? "toggle" : "sequence"}</div>
			<div class="list">
				{#each instance.children ?? [] as child, i (child.context)}
					<div class="list-item">
						<span class="muted">{i + 1}</span>
						<img class="icon small-icon" src={imageUrl(child.states[child.current_state]?.image ?? child.action.icon)} alt="" />
						<span class="grow">{child.action.name}</span>
						<button class="btn ghost danger" on:click={() => remove(child)} aria-label="Remove {child.action.name}">✕</button>
					</div>
				{/each}
				<button class="list-item accent" on:click={() => (mode = "child")}>＋ Add action</button>
			</div>
		{:else if instance.action.property_inspector && $device}
			<div class="section-title">Settings</div>
			{#key instance.context}
				<PropertyInspector {instance} device={$device} />
			{/key}
		{/if}

		<div class="section-title">Key</div>
		<div class="list">
			<button class="list-item" on:click={press}>▶︎ <span class="grow">Test press</span></button>
			<label class="list-item">
				⇄ <span class="grow">Show on every page</span>
				<input class="switch" type="checkbox" checked={isReplicated} on:change={toggleReplicated} />
			</label>
			<button class="list-item" on:click={() => dispatch("move", { slot, copy: false })}
				>↔︎ <span class="grow">Move to another {slot.controller == "Encoder" ? "dial" : "key"}</span></button
			>
			<button class="list-item" on:click={() => dispatch("move", { slot, copy: true })}
				>⧉ <span class="grow">Copy to another {slot.controller == "Encoder" ? "dial" : "key"}</span></button
			>
			<button class="list-item danger-text" on:click={() => instance && remove(instance)}>🗑 <span class="grow">Remove action</span></button>
		</div>
	</Sheet>
{/if}

<style>
	.summary {
		padding: 6px 0 16px;
	}
	.grow {
		flex: 1;
		min-width: 0;
	}
	.icon {
		width: 52px;
		height: 52px;
		border-radius: 12px;
		border: 1px solid var(--border);
		background: #000;
		object-fit: cover;
		flex: none;
	}
	.small-icon {
		width: 32px;
		height: 32px;
		border-radius: 8px;
	}
	.name {
		font-weight: 700;
		font-size: 17px;
	}
	.small {
		font-size: 13px;
		overflow-wrap: anywhere;
	}
	textarea {
		resize: none;
	}
	.toggle {
		min-height: 44px;
		margin-bottom: 4px;
	}
	.accent {
		color: var(--accent);
		font-weight: 600;
	}
	.danger-text {
		color: var(--danger);
	}
</style>
