<script lang="ts">
	import type { ActionInstance } from "$lib/ActionInstance";
	import type { Context } from "$lib/Context";
	import type { DeviceInfo } from "$lib/DeviceInfo";
	import type { DeviceLayout } from "$lib/DeviceLayout";
	import type { Profile } from "$lib/Profile";
	import type { CopiedItem } from "$lib/propertyInspector";

	import Key from "./Key.svelte";

	import { t } from "$lib/i18n";
	import { inspectedInstance, inspectedParentAction } from "$lib/propertyInspector";

	import { invoke } from "@tauri-apps/api/core";

	export let device: DeviceInfo;
	export let profile: Profile;

	export let selectedDevice: string;

	function handleDragStart({ dataTransfer }: DragEvent, controller: string, position: number) {
		if (!dataTransfer) return;
		dataTransfer.effectAllowed = "move";
		dataTransfer.setData("controller", controller);
		dataTransfer.setData("position", position.toString());
	}

	function handleDragOver(event: DragEvent) {
		event.preventDefault();
		if (!event.dataTransfer) return;
		if (event.dataTransfer.types.includes("action")) event.dataTransfer.dropEffect = "copy";
		else if (event.dataTransfer.types.includes("controller")) event.dataTransfer.dropEffect = "move";
	}

	async function handleDrop({ dataTransfer }: DragEvent, controller: string, position: number) {
		let context = { device: device.id, profile: profile.id, controller, position };
		let array = controller == "Encoder" ? profile.sliders : controller == "Infobar" ? profile.infobars : profile.keys;
		if (dataTransfer?.getData("action")) {
			let action = JSON.parse(dataTransfer?.getData("action"));
			if (array[position]) {
				return;
			}
			array[position] = await invoke("create_instance", { context, action });
			profile = profile;
		} else if (dataTransfer?.getData("controller")) {
			let oldController = dataTransfer?.getData("controller");
			let oldArray = oldController == "Encoder" ? profile.sliders : oldController == "Infobar" ? profile.infobars : profile.keys;
			let oldPosition = parseInt(dataTransfer?.getData("position"));
			let response: ActionInstance = await invoke("move_instance", {
				source: { device: device.id, profile: profile.id, controller: oldController, position: oldPosition },
				destination: context,
				retain: false,
			});
			if (response) {
				array[position] = response;
				oldArray[oldPosition] = null;
				profile = profile;
			}
		}
	}

	async function handlePaste(item: CopiedItem, destination: Context) {
		let array = destination.controller == "Encoder" ? profile.sliders : destination.controller == "Infobar" ? profile.infobars : profile.keys;

		if (item.type == "action") {
			if (array[destination.position]) return;
			array[destination.position] = await invoke("create_instance", { context: destination, action: item.action });
			profile = profile;
			return;
		}

		let response: ActionInstance = await invoke("move_instance", { source: item.source, destination, retain: true });
		if (response) {
			array[destination.position] = response;
			profile = profile;
		}
	}

	$: overflowsX = Math.max(device.columns, device.encoders, device.touchpoints) > 8;
	$: overflowsY = device.rows + Math.min(device.encoders, 1) + Math.min(device.touchpoints, 1) > 4;

	// Grid navigation: track focused cell and compute row lengths for arrow key movement.
	let focusedRow = 0;
	let focusedCol = 0;

	$: gridRowLengths = [
		...Array(device.rows).fill(device.columns),
		...(device.encoders > 0 ? [device.encoders] : []),
		...(device.touchpoints > 0 || device.infobars > 0 ? [device.touchpoints + device.infobars] : []),
	];
	$: encoderRowIndex = device.rows;
	$: touchpointRowIndex = device.rows + (device.encoders > 0 ? 1 : 0);
	$: keypadRowWidth = device.columns * 132;
	$: keySize = device.id.startsWith("sd-") && device.rows == 4 && device.columns == 8 ? 192 : 144;
	const layoutButtonClass = "px-3 py-1 text-sm text-neutral-300 bg-neutral-700 hover:bg-neutral-600 transition-colors border border-neutral-600 rounded-lg";

	function flatIndexFromRowCol(row: number, col: number): number {
		let index = 0;
		for (let r = 0; r < row; r++) index += gridRowLengths[r];
		return index + col;
	}

	function rowColFromFlatIndex(flatIndex: number): [number, number] {
		let remaining = flatIndex;
		for (let r = 0; r < gridRowLengths.length; r++) {
			if (remaining < gridRowLengths[r]) return [r, remaining];
			remaining -= gridRowLengths[r];
		}
		return [0, 0];
	}

	function handleGridKeydown(event: KeyboardEvent) {
		const target = event.target as HTMLElement;
		if (target.getAttribute("role") !== "gridcell") return;
		if (!["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;

		event.preventDefault();
		event.stopPropagation();

		let newRow = focusedRow;
		let newCol = focusedCol;

		switch (event.key) {
			case "ArrowRight":
				newCol = Math.min(focusedCol + 1, gridRowLengths[focusedRow] - 1);
				break;
			case "ArrowLeft":
				newCol = Math.max(focusedCol - 1, 0);
				break;
			case "ArrowDown":
				newRow = Math.min(focusedRow + 1, gridRowLengths.length - 1);
				newCol = Math.min(focusedCol, gridRowLengths[newRow] - 1);
				break;
			case "ArrowUp":
				newRow = Math.max(focusedRow - 1, 0);
				newCol = Math.min(focusedCol, gridRowLengths[newRow] - 1);
				break;
			case "Home":
				newCol = 0;
				break;
			case "End":
				newCol = gridRowLengths[focusedRow] - 1;
				break;
		}

		if (newRow === focusedRow && newCol === focusedCol) return;

		focusedRow = newRow;
		focusedCol = newCol;

		const grid = event.currentTarget as HTMLElement;
		const cells = grid.querySelectorAll("[role='gridcell']");
		(cells[flatIndexFromRowCol(newRow, newCol)] as HTMLElement)?.focus();
	}

	// Custom layouts: controllers can be placed anywhere, in units of one key slot.
	const SLOT = 132;
	const SNAP = 0.25;
	const MIN_SCALE = 0.5;
	const MAX_SCALE = 2.5;
	const INFOBAR_WIDTH = 2;

	type LayoutItem = { id: string; controller: "Keypad" | "Encoder" | "Infobar"; position: number; width: number; height: number; isTouchPoint: boolean };

	let layout: DeviceLayout | null = null;
	let editingLayout = false;
	let draft: DeviceLayout = {};
	let drag: { id: string; pointerX: number; pointerY: number; x: number; y: number } | null = null;

	invoke<DeviceLayout | null>("get_device_layout", { device: device.id }).then((value) => (layout = value));

	// Same order as the default grid, so keyboard navigation visits controllers in the same order.
	function buildLayoutItems(device: DeviceInfo): LayoutItem[] {
		const items: LayoutItem[] = [];
		const keypad = (position: number, isTouchPoint = false) => ({
			id: `Keypad.${position}`,
			controller: "Keypad" as const,
			position,
			width: 1,
			height: 1,
			isTouchPoint,
		});
		for (let i = 0; i < device.rows * device.columns; i++) items.push(keypad(i));
		for (let i = 0; i < device.encoders; i++) items.push({ id: `Encoder.${i}`, controller: "Encoder", position: i, width: 1, height: 1, isTouchPoint: false });
		for (let i = 0; i < device.touchpoints; i++) {
			if (device.infobars > 0 && i === 1) {
				for (let j = 0; j < device.infobars; j++) {
					items.push({ id: `Infobar.${j}`, controller: "Infobar", position: j, width: INFOBAR_WIDTH, height: 0.5, isTouchPoint: false });
				}
			}
			items.push(keypad(device.rows * device.columns + i, true));
		}
		return items;
	}

	// Positions matching the default grid, used as the starting point for a new layout.
	function defaultLayout(device: DeviceInfo): DeviceLayout {
		const positions: DeviceLayout = {};
		for (let r = 0; r < device.rows; r++) {
			for (let c = 0; c < device.columns; c++) positions[`Keypad.${r * device.columns + c}`] = { x: c, y: r };
		}
		for (let i = 0; i < device.encoders; i++) {
			positions[`Encoder.${i}`] = { x: device.encoders > 1 ? (i * Math.max(device.columns - 1, 0)) / (device.encoders - 1) : 0, y: device.rows };
		}
		const y = device.rows + (device.encoders > 0 ? 1 : 0);
		let x = 0;
		for (let i = 0; i < device.touchpoints; i++) {
			if (device.infobars > 0 && i === 1) {
				for (let j = 0; j < device.infobars; j++) {
					positions[`Infobar.${j}`] = { x, y: y + 0.25 };
					x += INFOBAR_WIDTH;
				}
			}
			positions[`Keypad.${device.rows * device.columns + i}`] = { x, y };
			x += 1;
		}
		return positions;
	}

	$: layoutItems = buildLayoutItems(device);
	$: defaults = defaultLayout(device);
	$: positions = editingLayout ? draft : (layout ?? defaults);
	function itemExtent(item: LayoutItem, axis: "x" | "y") {
		const position = positions[item.id] ?? defaults[item.id];
		return position[axis] + (axis == "x" ? item.width : item.height) * (position.scale ?? 1);
	}
	$: extentX = Math.max(1, ...layoutItems.map((item) => itemExtent(item, "x")));
	$: extentY = Math.max(1, ...layoutItems.map((item) => itemExtent(item, "y")));

	function startEditingLayout() {
		draft = { ...defaults, ...(layout ?? {}) };
		editingLayout = true;
	}

	function cancelEditingLayout() {
		editingLayout = false;
		drag = null;
	}

	async function saveLayout() {
		const isDefault = layoutItems.every(
			(item) => draft[item.id]?.x === defaults[item.id].x && draft[item.id]?.y === defaults[item.id].y && (draft[item.id]?.scale ?? 1) === 1,
		);
		const value = isDefault ? null : draft;
		await invoke("set_device_layout", { device: device.id, layout: value });
		layout = value;
		editingLayout = false;
		drag = null;
	}

	function startLayoutDrag(event: PointerEvent, id: string) {
		event.preventDefault();
		event.stopPropagation();
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		const position = draft[id] ?? defaults[id];
		drag = { id, pointerX: event.clientX, pointerY: event.clientY, x: position.x, y: position.y };
	}

	function moveLayoutDrag(event: PointerEvent) {
		if (!drag) return;
		const snap = (value: number) => Math.max(0, Math.round(value / SNAP) * SNAP);
		draft[drag.id] = {
			...draft[drag.id],
			x: snap(drag.x + (event.clientX - drag.pointerX) / SLOT),
			y: snap(drag.y + (event.clientY - drag.pointerY) / SLOT),
		};
	}

	function resizeLayoutItem(event: WheelEvent, id: string) {
		const position = draft[id] ?? defaults[id];
		const step = event.deltaY < 0 ? SNAP : -SNAP;
		const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, (position.scale ?? 1) + step));
		draft[id] = { ...position, scale };
	}

	function handleGridFocusin(event: FocusEvent) {
		const grid = event.currentTarget as HTMLElement;
		const cells = Array.from(grid.querySelectorAll("[role='gridcell']"));
		const index = cells.indexOf(event.target as Element);
		if (index === -1) return;
		[focusedRow, focusedCol] = rowColFromFlatIndex(index);
	}
</script>

{#key device}
	<span id="grid-description" class="sr-only">{$t("device_view.grid_description")}</span>
	<div
		class="relative flex flex-col justify-center grow px-16 pt-14 pb-6 overflow-auto"
		class:items-center={device.columns <= 9}
		class:hidden={$inspectedParentAction || selectedDevice != device.id}
		class:device-fade-x={overflowsX && !overflowsY}
		class:device-fade-y={overflowsY && !overflowsX}
		class:device-fade-xy={overflowsX && overflowsY}
		role="grid"
		aria-label={device.name}
		aria-describedby="grid-description"
		tabindex="-1"
		on:click={() => inspectedInstance.set(null)}
		on:keyup={() => inspectedInstance.set(null)}
		on:keydown|capture={handleGridKeydown}
		on:focusin={handleGridFocusin}
	>
		<div class="absolute top-2 right-4 z-10 flex flex-row space-x-2">
			{#if editingLayout}
				<span class="self-center text-sm text-neutral-400">{$t("device_view.layout_hint")}</span>
				<button class={layoutButtonClass} on:click|stopPropagation={() => (draft = { ...defaults })}>{$t("device_view.layout_reset")}</button>
				<button class={layoutButtonClass} on:click|stopPropagation={cancelEditingLayout}>{$t("device_view.layout_cancel")}</button>
				<button class={layoutButtonClass} on:click|stopPropagation={saveLayout}>{$t("device_view.layout_done")}</button>
			{:else}
				<button class={layoutButtonClass} on:click|stopPropagation={startEditingLayout}>{$t("device_view.layout_edit")}</button>
			{/if}
		</div>

		{#if layout || editingLayout}
			<div class="relative" role="rowgroup" style={`width: ${extentX * SLOT}px; height: ${extentY * SLOT}px;`}>
				{#each layoutItems as item, index (item.id)}
					{@const position = positions[item.id] ?? defaults[item.id]}
					<div
						class="absolute origin-top-left"
						role="row"
						style={`left: ${position.x * SLOT}px; top: ${position.y * SLOT}px; transform: scale(${position.scale ?? 1});`}
					>
						{#if item.controller == "Encoder"}
							<Key
								context={{ device: device.id, profile: profile.id, controller: "Encoder", position: item.position }}
								bind:inslot={profile.sliders[item.position]}
								on:dragover={handleDragOver}
								on:drop={(event) => handleDrop(event, "Encoder", item.position)}
								on:dragstart={(event) => handleDragStart(event, "Encoder", item.position)}
								{handlePaste}
								size={keySize}
								label="{$t('device_view.encoder')} {item.position + 1}"
								tabindex={flatIndexFromRowCol(focusedRow, focusedCol) === index ? 0 : -1}
							/>
						{:else if item.controller == "Infobar"}
							<div class="px-3.5 py-[3.5px]">
								<Key
									context={{ device: device.id, profile: profile.id, controller: "Infobar", position: item.position }}
									bind:inslot={profile.infobars[item.position]}
									on:dragover={handleDragOver}
									on:drop={(event) => handleDrop(event, "Infobar", item.position)}
									on:dragstart={(event) => handleDragStart(event, "Infobar", item.position)}
									{handlePaste}
									size={keySize}
									width={248}
									height={58}
								/>
							</div>
						{:else}
							<Key
								context={{ device: device.id, profile: profile.id, controller: "Keypad", position: item.position }}
								bind:inslot={profile.keys[item.position]}
								on:dragover={handleDragOver}
								on:drop={(event) => handleDrop(event, "Keypad", item.position)}
								on:dragstart={(event) => handleDragStart(event, "Keypad", item.position)}
								{handlePaste}
								size={keySize}
								isTouchPoint={item.isTouchPoint}
								label={item.isTouchPoint
									? `${$t("device_view.touchpoint")} ${item.position - device.rows * device.columns + 1}`
									: `${$t("device_view.key")} ${String.fromCharCode(65 + Math.floor(item.position / device.columns))}${(item.position % device.columns) + 1}`}
								tabindex={flatIndexFromRowCol(focusedRow, focusedCol) === index ? 0 : -1}
							/>
						{/if}
						{#if editingLayout}
							<div
								class="absolute inset-1 rounded-lg border-2 border-dashed border-blue-400 bg-blue-400/10 touch-none"
								class:cursor-grab={drag?.id !== item.id}
								class:cursor-grabbing={drag?.id === item.id}
								role="presentation"
								on:pointerdown={(event) => startLayoutDrag(event, item.id)}
								on:pointermove={moveLayoutDrag}
								on:pointerup={() => (drag = null)}
								on:pointercancel={() => (drag = null)}
								on:wheel|preventDefault={(event) => resizeLayoutItem(event, item.id)}
								on:click|stopPropagation
							></div>
						{/if}
					</div>
				{/each}
			</div>
		{:else}
			<div class="flex flex-col" role="rowgroup">
				{#each { length: device.rows } as _, r}
					<div class="flex flex-row" role="row">
						{#each { length: device.columns } as _, c}
							<Key
								context={{ device: device.id, profile: profile.id, controller: "Keypad", position: r * device.columns + c }}
								bind:inslot={profile.keys[r * device.columns + c]}
								on:dragover={handleDragOver}
								on:drop={(event) => handleDrop(event, "Keypad", r * device.columns + c)}
								on:dragstart={(event) => handleDragStart(event, "Keypad", r * device.columns + c)}
								{handlePaste}
								size={device.id.startsWith("sd-") && device.rows == 4 && device.columns == 8 ? 192 : 144}
								label="{$t('device_view.key')} {String.fromCharCode(65 + r)}{c + 1}"
								tabindex={focusedRow === r && focusedCol === c ? 0 : -1}
							/>
						{/each}
					</div>
				{/each}
			</div>

			<div class="flex flex-row justify-between" role="row" style={`width: ${keypadRowWidth}px;`}>
				{#each { length: device.encoders } as _, i}
					<Key
						context={{ device: device.id, profile: profile.id, controller: "Encoder", position: i }}
						bind:inslot={profile.sliders[i]}
						on:dragover={handleDragOver}
						on:drop={(event) => handleDrop(event, "Encoder", i)}
						on:dragstart={(event) => handleDragStart(event, "Encoder", i)}
						{handlePaste}
						size={device.id.startsWith("sd-") && device.rows == 4 && device.columns == 8 ? 192 : 144}
						label="{$t('device_view.encoder')} {i + 1}"
						tabindex={focusedRow === encoderRowIndex && focusedCol === i ? 0 : -1}
					/>
				{/each}
			</div>

			<div class="flex flex-row items-center" role="row">
				{#each { length: device.touchpoints } as _, i}
					<!-- On the Stream Deck Neo, the infobar display sits physically between the two touchpoints. -->
					{#if device.infobars > 0 && i === 1}
						{#each { length: device.infobars } as _, j}
							<div class="px-3.5 py-[3.5px]">
								<Key
									context={{ device: device.id, profile: profile.id, controller: "Infobar", position: j }}
									bind:inslot={profile.infobars[j]}
									on:dragover={handleDragOver}
									on:drop={(event) => handleDrop(event, "Infobar", j)}
									on:dragstart={(event) => handleDragStart(event, "Infobar", j)}
									{handlePaste}
									size={device.id.startsWith("sd-") && device.rows == 4 && device.columns == 8 ? 192 : 144}
									width={248}
									height={58}
								/>
							</div>
						{/each}
					{/if}
					<Key
						context={{ device: device.id, profile: profile.id, controller: "Keypad", position: device.rows * device.columns + i }}
						bind:inslot={profile.keys[device.rows * device.columns + i]}
						on:dragover={handleDragOver}
						on:drop={(event) => handleDrop(event, "Keypad", device.rows * device.columns + i)}
						on:dragstart={(event) => handleDragStart(event, "Keypad", device.rows * device.columns + i)}
						{handlePaste}
						size={device.id.startsWith("sd-") && device.rows == 4 && device.columns == 8 ? 192 : 144}
						isTouchPoint
						label="{$t('device_view.touchpoint')} {i + 1}"
						tabindex={focusedRow === touchpointRowIndex && focusedCol === i ? 0 : -1}
					/>
				{/each}
			</div>
		{/if}
	</div>
{/key}

<style>
	.device-fade-x {
		mask-image: linear-gradient(to right, transparent, black 7.5rem, black calc(100% - 7.5rem), transparent);
	}
	.device-fade-y {
		mask-image: linear-gradient(to bottom, transparent, black 7.5rem, black calc(100% - 7.5rem), transparent);
	}
	.device-fade-xy {
		mask-image:
			linear-gradient(to right, transparent, black 7.5rem, black calc(100% - 7.5rem), transparent),
			linear-gradient(to bottom, transparent, black 7.5rem, black calc(100% - 7.5rem), transparent);
		mask-composite: intersect;
	}
</style>
