import type { Action } from "../../src/lib/Action.ts";
import type { ActionInstance } from "../../src/lib/ActionInstance.ts";
import type { Context } from "../../src/lib/Context.ts";
import type { DeviceInfo } from "../../src/lib/DeviceInfo.ts";
import type { LayoutPosition } from "../../src/lib/DeviceLayout.ts";
import type { Profile } from "../../src/lib/Profile.ts";

import { type DeckEvent, EventStream, invoke } from "./api.ts";

import { derived, get, writable } from "svelte/store";

export type { Action, ActionInstance, Context, DeviceInfo, LayoutPosition, Profile };
export type Category = { icon: string | null; actions: Action[] };
export type PluginInfo = {
	id: string;
	name: string;
	author: string;
	icon: string;
	version: string;
	has_settings_interface: boolean;
	builtin: boolean;
	registered: boolean;
};

export type Controller = "Keypad" | "Encoder";
export type Slot = { controller: Controller; position: number };

export const connected = writable(false);
export const devices = writable<Record<string, DeviceInfo>>({});
export const deviceId = writable<string | null>(null);
export const profiles = writable<string[]>([]);
export const profile = writable<Profile | null>(null);
export const layout = writable<Record<string, LayoutPosition> | null>(null);
export const images = writable<Record<string, string>>({});
export const replicated = writable<string[]>([]);
export const categories = writable<Record<string, Category>>({});
export const plugins = writable<PluginInfo[]>([]);
export const pressed = writable<Record<string, boolean>>({});
export const toast = writable<{ text: string; kind: "info" | "error" } | null>(null);

export const device = derived([devices, deviceId], ([$devices, $deviceId]) => ($deviceId ? ($devices[$deviceId] ?? null) : null));

export function slotKey(controller: string, position: number): string {
	return `${controller}.${position}`;
}

export function instanceAt(p: Profile | null, slot: Slot): ActionInstance | null {
	if (!p) return null;
	return (slot.controller == "Encoder" ? p.sliders[slot.position] : p.keys[slot.position]) ?? null;
}

export function contextFor(slot: Slot): Context {
	const p = get(profile);
	return { device: get(deviceId)!, profile: p!.id, controller: slot.controller, position: slot.position };
}

let toastTimer: ReturnType<typeof setTimeout> | null = null;
export function notify(text: string, kind: "info" | "error" = "info") {
	toast.set({ text, kind });
	if (toastTimer) clearTimeout(toastTimer);
	toastTimer = setTimeout(() => toast.set(null), kind == "error" ? 5000 : 2500);
}

/// Run a command, reporting failures as a toast. Returns undefined on failure.
export async function attempt<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T | undefined> {
	try {
		return await invoke<T>(cmd, args);
	} catch (error: any) {
		notify(error?.message ?? String(error), "error");
		return undefined;
	}
}

const DEVICE_KEY = "opendeck-remote-device";

export async function loadDevices() {
	const list: Record<string, DeviceInfo> = await invoke("get_devices");
	devices.set(list);
	const ids = Object.keys(list);
	let current = get(deviceId);
	if (!current || !list[current]) {
		let saved: string | null = null;
		try {
			saved = localStorage.getItem(DEVICE_KEY);
		} catch {}
		current = saved && list[saved] ? saved : (ids[0] ?? null);
		deviceId.set(current);
	}
}

export function selectDevice(id: string) {
	deviceId.set(id);
	try {
		localStorage.setItem(DEVICE_KEY, id);
	} catch {}
	refreshDevice();
}

/// Reload everything shown for the selected device.
export async function refreshDevice() {
	const id = get(deviceId);
	if (!id) {
		profile.set(null);
		return;
	}
	const [p, list, l, imgs, rep] = await Promise.all([
		invoke<Profile>("get_selected_profile", { device: id }),
		invoke<string[]>("get_profiles", { device: id }),
		invoke<Record<string, LayoutPosition> | null>("get_device_layout", { device: id }).catch(() => null),
		invoke<Record<string, string>>("get_rendered_images", { device: id }).catch(() => ({})),
		invoke<string[]>("get_replicated_slots", { device: id }).catch(() => []),
	]);
	if (get(deviceId) != id) return;
	profile.set(p);
	profiles.set(list);
	layout.set(l);
	images.set(imgs);
	replicated.set(rep);
}

export async function loadCatalogue() {
	const [c, p] = await Promise.all([invoke<Record<string, Category>>("get_categories"), invoke<PluginInfo[]>("list_plugins").catch(() => [])]);
	categories.set(c);
	plugins.set(p);
}

export async function refreshAll() {
	await loadDevices();
	await Promise.all([refreshDevice(), loadCatalogue()]);
}

let refreshTimer: ReturnType<typeof setTimeout> | null = null;
function scheduleRefresh(delay = 150) {
	if (refreshTimer) clearTimeout(refreshTimer);
	refreshTimer = setTimeout(() => {
		refreshTimer = null;
		refreshDevice().catch(() => {});
	}, delay);
}

function handleEvent({ event, payload }: DeckEvent) {
	const id = get(deviceId);
	switch (event) {
		case "resync":
			refreshAll().catch(() => {});
			break;
		case "devices":
			devices.set(payload ?? {});
			loadDevices().then(() => scheduleRefresh());
			break;
		case "key_image":
			if (payload?.device == id && payload.profile == get(profile)?.id) {
				images.update((all) => {
					if (payload.image) all[payload.slot] = payload.image;
					else delete all[payload.slot];
					return all;
				});
			}
			break;
		case "key_moved":
			if (payload?.context?.device == id) {
				pressed.update((all) => ({ ...all, [slotKey(payload.context.controller, payload.context.position)]: payload.pressed }));
			}
			break;
		case "replicated_slots":
			if (payload?.[0] == id) replicated.set(payload[1]);
			break;
		case "plugin_reloaded":
			loadCatalogue().catch(() => {});
			scheduleRefresh();
			break;
		case "update_state":
		case "switch_profile":
		case "rerender_images":
		case "profile_changed":
		case "profiles_changed":
			// Profile switches go through the desktop frontend first, so give it a moment.
			scheduleRefresh(event == "switch_profile" ? 400 : 150);
			break;
	}
}

let stream: EventStream | null = null;
export function startEvents() {
	stream?.close();
	stream = new EventStream(handleEvent, (value) => connected.set(value));
}
export function stopEvents() {
	stream?.close();
	stream = null;
}
