<script lang="ts">
	import { attempt, deviceId, notify, profile, profiles, refreshDevice } from "../state.ts";
	import Sheet from "./Sheet.svelte";
	import { createEventDispatcher } from "svelte";

	const dispatch = createEventDispatcher<{ close: void }>();

	let editing = false;
	let newName = "";
	let renaming: string | null = null;
	let renameValue = "";
	let busy = false;

	$: device = $deviceId!;
	$: current = $profile?.id;

	async function run(fn: () => Promise<unknown>) {
		if (busy) return;
		busy = true;
		try {
			await fn();
			await refreshDevice();
		} finally {
			busy = false;
		}
	}

	const switchTo = (id: string) =>
		run(async () => {
			if (id != current) await attempt("set_selected_profile", { device, id });
			if (!editing) dispatch("close");
		});

	const create = () =>
		run(async () => {
			const id = newName.trim();
			if (!id) return;
			if ($profiles.includes(id)) return notify(`A page named ${id} already exists.`, "error");
			// Selecting a profile that does not exist yet creates it.
			if ((await attempt("set_selected_profile", { device, id })) !== undefined) {
				newName = "";
				dispatch("close");
			}
		});

	const rename = (oldId: string) =>
		run(async () => {
			const newId = renameValue.trim();
			renaming = null;
			if (!newId || newId == oldId) return;
			if ($profiles.includes(newId)) return notify(`A page named ${newId} already exists.`, "error");
			await attempt("rename_profile", { device, oldId, newId, retain: false });
		});

	const duplicate = (id: string) =>
		run(async () => {
			let copy = `${id} copy`;
			for (let n = 2; $profiles.includes(copy); n++) copy = `${id} copy ${n}`;
			await attempt("rename_profile", { device, oldId: id, newId: copy, retain: true });
		});

	const remove = (id: string) =>
		run(async () => {
			if (!confirmDelete || confirmDelete != id) {
				confirmDelete = id;
				return;
			}
			confirmDelete = null;
			await attempt("delete_profile", { device, profile: id });
		});
	let confirmDelete: string | null = null;

	const move = (index: number, by: number) =>
		run(async () => {
			const order = [...$profiles];
			const target = index + by;
			if (target < 0 || target >= order.length) return;
			[order[index], order[target]] = [order[target], order[index]];
			profiles.set(order);
			await attempt("set_profile_order", { device, order });
		});
</script>

<Sheet title="Pages" on:close={() => dispatch("close")}>
	<div class="row head">
		<span class="muted grow">{$profiles.length} {$profiles.length == 1 ? "page" : "pages"} on this deck</span>
		<button class="btn ghost accent" on:click={() => ((editing = !editing), (renaming = null), (confirmDelete = null))}>{editing ? "Done" : "Edit"}</button>
	</div>
	<div class="list">
		{#each $profiles as id, i (id)}
			<div class="list-item" class:current={id == current}>
				{#if renaming == id}
					<form class="row grow" on:submit|preventDefault={() => rename(id)}>
						<!-- svelte-ignore a11y-autofocus -->
						<input class="input grow" bind:value={renameValue} autofocus />
						<button class="btn primary" type="submit">Save</button>
					</form>
				{:else}
					<button class="name grow" on:click={() => switchTo(id)} disabled={busy}>
						<span class="check">{id == current ? "●" : ""}</span>
						{id}
					</button>
					{#if editing}
						<div class="tools">
							<button class="btn ghost" aria-label="Move {id} up" disabled={i == 0} on:click={() => move(i, -1)}>↑</button>
							<button class="btn ghost" aria-label="Move {id} down" disabled={i == $profiles.length - 1} on:click={() => move(i, 1)}>↓</button>
							<button class="btn ghost" aria-label="Rename {id}" on:click={() => ((renaming = id), (renameValue = id))}>✎</button>
							<button class="btn ghost" aria-label="Duplicate {id}" on:click={() => duplicate(id)}>⧉</button>
							{#if id != current}
								<button class="btn ghost danger" aria-label="Delete {id}" on:click={() => remove(id)}>{confirmDelete == id ? "Delete?" : "🗑"}</button>
							{/if}
						</div>
					{/if}
				{/if}
			</div>
		{/each}
	</div>

	<div class="section-title">New page</div>
	<form class="row" on:submit|preventDefault={create}>
		<input class="input grow" bind:value={newName} placeholder="Page name" />
		<button class="btn primary" type="submit" disabled={!newName.trim() || busy}>Add</button>
	</form>
	<p class="muted hint">Use a slash to put a page in a folder, like <em>Rooms/Kitchen</em>.</p>
</Sheet>

<style>
	.head {
		margin-bottom: 8px;
	}
	.grow {
		flex: 1;
		min-width: 0;
	}
	.accent {
		color: var(--accent);
	}
	.name {
		display: flex;
		align-items: center;
		gap: 8px;
		background: none;
		border: none;
		text-align: left;
		padding: 8px 0;
		min-height: 44px;
		overflow-wrap: anywhere;
	}
	.check {
		width: 14px;
		color: var(--accent);
		font-size: 12px;
		flex: none;
	}
	.current .name {
		font-weight: 700;
	}
	.tools {
		display: flex;
		flex: none;
	}
	.tools .btn {
		padding: 0 9px;
	}
	.hint {
		font-size: 13px;
	}
</style>
