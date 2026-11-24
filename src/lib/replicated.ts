import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { type Writable, writable } from "svelte/store";

/// Replicated slots per device, as `Controller.position` keys (e.g. `Keypad.3`).
export const replicatedSlots: Writable<{ [device: string]: string[] }> = writable({});

export function slotKey(controller: string, position: number): string {
	return `${controller}.${position}`;
}

export async function refreshReplicated(device: string) {
	try {
		const slots: string[] = await invoke("get_replicated_slots", { device });
		replicatedSlots.update((all) => ({ ...all, [device]: slots }));
	} catch {}
}

const requested = new Set<string>();
export function ensureReplicatedLoaded(device: string) {
	if (requested.has(device)) return;
	requested.add(device);
	refreshReplicated(device);
}

listen("replicated_slots", ({ payload }: { payload: [string, string[]] }) => {
	replicatedSlots.update((all) => ({ ...all, [payload[0]]: payload[1] }));
});
