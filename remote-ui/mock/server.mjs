// Mock of OpenDeck's remote API (src-tauri/src/remote) for developing and testing the mobile app without a deck.
// It emulates a Streonor S6 with the Home Assistant plugin installed. No dependencies: `node remote-ui/mock/server.mjs`.
//
// Environment: PORT (default 57200), TOKEN (default "mocktoken0123456789"), DIST (built app, default remote-ui/dist).

import { createHash } from "node:crypto";
import { readFileSync, existsSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, extname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const PORT = Number(process.env.PORT ?? 57200);
const TOKEN = process.env.TOKEN ?? "mocktoken0123456789";
const DIST = process.env.DIST ?? join(here, "..", "dist");
const PLUGIN_DIR = "/home/pi/.config/opendeck/plugins";

// ---- Model ----

const DEVICE = { id: "sn-S6MOCK01", name: "Streonor S6", rows: 3, columns: 3, encoders: 3, touchpoints: 0, infobars: 0, type: 7 };
// Six LCD keys on top, the three plain buttons under them, the big dial and the two small dials at the bottom.
const LAYOUT = {
	"Keypad.0": { x: 0, y: 0 },
	"Keypad.1": { x: 1, y: 0 },
	"Keypad.2": { x: 2, y: 0 },
	"Keypad.3": { x: 0, y: 1 },
	"Keypad.4": { x: 1, y: 1 },
	"Keypad.5": { x: 2, y: 1 },
	"Keypad.6": { x: 0.15, y: 2, scale: 0.7 },
	"Keypad.7": { x: 1.15, y: 2, scale: 0.7 },
	"Keypad.8": { x: 2.15, y: 2, scale: 0.7 },
	"Encoder.0": { x: 0.9, y: 2.75, scale: 1.2 },
	"Encoder.1": { x: 0, y: 3.2 },
	"Encoder.2": { x: 2, y: 3.2 },
};

const state = (overrides = {}) => ({
	image: "",
	image_override: null,
	image_scale: 100,
	background_colour: "#000000",
	name: "",
	text: "",
	text_override: null,
	show: true,
	colour: "#FFFFFF",
	stroke_colour: "#000000",
	alignment: "bottom",
	family: "Liberation Sans",
	style: "Regular",
	size: 16,
	stroke_size: 0,
	underline: false,
	...overrides,
});

const icon = (glyph, bg) =>
	"data:image/svg+xml," +
	encodeURIComponent(
		`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 144 144"><rect width="144" height="144" rx="20" fill="${bg}"/><text x="72" y="96" font-size="72" text-anchor="middle" fill="#fff" font-family="sans-serif">${glyph}</text></svg>`,
	);

const action = (plugin, uuid, name, glyph, bg, controllers = ["Keypad"], extra = {}) => ({
	name,
	uuid,
	plugin,
	tooltip: extra.tooltip ?? "",
	icon: icon(glyph, bg),
	visible_in_action_list: true,
	supported_in_multi_actions: !uuid.startsWith("opendeck."),
	property_inspector: extra.pi ?? "",
	controllers,
	encoder: controllers.includes("Encoder")
		? { icon: icon(glyph, bg), stack_color: "", trigger_description: { long_touch: "", push: "", rotate: "", touch: "" }, background: "", layout: "$X1" }
		: null,
	states: [state({ image: icon(glyph, bg), name })],
});

const HA = "io.github.gauravk-in.homeassistant";
const HA_PI = `${PLUGIN_DIR}/${HA}.sdPlugin/propertyInspector/settings.html`;
const CATEGORIES = {
	"Home Assistant": {
		icon: null,
		actions: [
			action(HA, `${HA}.deck`, "Home Assistant", "⌂", "#18bcf2", ["Keypad", "Encoder"], {
				tooltip: "Rooms, lights and climate from Home Assistant",
				pi: HA_PI,
			}),
		],
	},
	"Page Switcher": {
		icon: null,
		actions: [
			action("io.github.gauravk-in.pageswitcher", "io.github.gauravk-in.pageswitcher.next", "Next page", "→", "#6d28d9", ["Keypad", "Encoder"]),
			action("io.github.gauravk-in.pageswitcher", "io.github.gauravk-in.pageswitcher.previous", "Previous page", "←", "#6d28d9"),
		],
	},
	"Web Link": {
		icon: null,
		actions: [
			action("io.github.gauravk-in.weblink", "io.github.gauravk-in.weblink.open", "Open website", "🌐", "#0f766e", ["Keypad"], { tooltip: "Open a website" }),
		],
	},
	OpenDeck: {
		icon: null,
		actions: [
			{ ...action("opendeck", "opendeck.multiaction", "Multi Action", "≡", "#525252"), children: [] },
			{ ...action("opendeck", "opendeck.toggleaction", "Toggle Action", "⇆", "#525252"), children: [] },
		],
	},
};
const PLUGINS = [
	{
		id: HA,
		name: "Home Assistant",
		author: "gauravk-in",
		icon: icon("⌂", "#18bcf2"),
		version: "0.1.0",
		has_settings_interface: false,
		builtin: false,
		registered: true,
	},
	{
		id: "io.github.gauravk-in.pageswitcher",
		name: "Page Switcher",
		author: "gauravk-in",
		icon: icon("→", "#6d28d9"),
		version: "0.1.0",
		has_settings_interface: false,
		builtin: false,
		registered: true,
	},
	{
		id: "io.github.gauravk-in.weblink",
		name: "Web Link",
		author: "gauravk-in",
		icon: icon("🌐", "#0f766e"),
		version: "0.2.0",
		has_settings_interface: false,
		builtin: false,
		registered: true,
	},
];

const findAction = (uuid) =>
	Object.values(CATEGORIES)
		.flatMap((c) => c.actions)
		.find((a) => a.uuid == uuid);
const emptyProfile = (id) => ({ device: DEVICE.id, id, keys: Array(9).fill(null), sliders: Array(3).fill(null), infobars: [] });
const ctx = (c, index = 0) => `${c.device}.${c.profile}.${c.controller}.${c.position}.${index}`;
const parseCtx = (s) => {
	const parts = s.split(".");
	const index = Number(parts.pop()),
		position = Number(parts.pop()),
		controller = parts.pop();
	const device = parts.shift();
	return { device, profile: parts.join("."), controller, position, index };
};
function instance(a, c, text) {
	const inst = {
		action: a,
		context: ctx(c),
		states: structuredClone(a.states),
		current_state: 0,
		settings: {},
		children: a.uuid.startsWith("opendeck.") ? [] : null,
	};
	if (text != null) inst.states[0].text = text;
	return inst;
}

const profiles = new Map();
let order = ["Home Assistant", "Default"];
let selected = "Home Assistant";
let replicated = ["Encoder.0"];
let globalSettings = { [HA]: { url: "http://homeassistant.local:8123", token: "", timeoutSecs: 30, hiddenAreas: [], hiddenEntities: [] } };
let settings = {
	version: "2.14.2",
	language: "en",
	brightness: 60,
	sleep_timeout_minutes: 5,
	sleep_when_computer_locked: false,
	rotation: 0,
	background: true,
	autolaunch: true,
	updatecheck: false,
	statistics: false,
	separatewine: false,
	developer: false,
	disableelgato: false,
};

{
	const p = emptyProfile("Home Assistant");
	const c = (controller, position) => ({ device: DEVICE.id, profile: p.id, controller, position });
	const ha = findAction(`${HA}.deck`);
	["Living room", "Kitchen", "Bedroom", "Office", "Bathroom", "Garden"].forEach((room, i) => (p.keys[i] = instance(ha, c("Keypad", i), room)));
	p.keys[7] = instance(findAction("io.github.gauravk-in.pageswitcher.next"), c("Keypad", 7), "");
	p.sliders[0] = instance(ha, c("Encoder", 0), "Temperature");
	p.sliders[1] = instance(ha, c("Encoder", 1), "Brightness");
	profiles.set(p.id, p);
	const d = emptyProfile("Default");
	d.sliders[0] = structuredClone(p.sliders[0]);
	d.sliders[0].context = ctx({ ...c("Encoder", 0), profile: "Default" });
	profiles.set("Default", d);
}

const getProfile = (id) => {
	if (!profiles.has(id)) {
		profiles.set(id, emptyProfile(id));
		if (!order.includes(id)) order.push(id);
	}
	return profiles.get(id);
};
const slotArray = (p, controller) => (controller == "Encoder" ? p.sliders : p.keys);

// ---- Rendering, standing in for the desktop frontend that renders key images ----

const esc = (s) => String(s).replace(/[<>&"]/g, (ch) => ({ "<": "&lt;", ">": "&gt;", "&": "&amp;", '"': "&quot;" })[ch]);
function render(inst) {
	const s = inst.states[inst.current_state];
	const title = s.show ? (s.text_override ?? s.text) : "";
	const img = s.image_override ?? s.image;
	const y = { top: 30, middle: 80, bottom: 130 }[s.alignment] ?? 130;
	const svg = `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 144 144"><rect width="144" height="144" fill="${esc(s.background_colour || "#000")}"/><image href="${esc(img)}" width="144" height="144"/>${title ? `<text x="72" y="${y}" font-size="20" font-weight="bold" text-anchor="middle" fill="${esc(s.colour)}" stroke="#000" stroke-width="3" paint-order="stroke" font-family="sans-serif">${esc(title)}</text>` : ""}</svg>`;
	return "data:image/svg+xml," + encodeURIComponent(svg);
}
let images = {};
function rerender() {
	const p = getProfile(selected);
	const next = {};
	p.keys.forEach((inst, i) => inst && (next[`Keypad.${i}`] = render(inst)));
	for (const slot of new Set([...Object.keys(images), ...Object.keys(next)])) {
		if (images[slot] != next[slot]) broadcast("key_image", { device: DEVICE.id, profile: selected, slot, image: next[slot] ?? null });
	}
	images = next;
}

function propagateReplicated(from) {
	const source = getProfile(from);
	for (const [id, p] of profiles) {
		if (id == from) continue;
		for (const key of replicated) {
			const [controller, pos] = key.split(".");
			const inst = slotArray(source, controller)[pos];
			slotArray(p, controller)[pos] = inst
				? { ...structuredClone(inst), context: ctx({ device: DEVICE.id, profile: id, controller, position: Number(pos) }) }
				: null;
		}
	}
}

// ---- Commands ----

const commands = {
	get_devices: () => ({ [DEVICE.id]: DEVICE }),
	get_port_base: () => 57116,
	get_categories: () => CATEGORIES,
	get_localisations: () => ({}),
	get_device_layout: () => LAYOUT,
	set_device_layout: () => null,
	get_fonts: () => ["Liberation Sans"],
	get_build_info: () => "mock",
	list_plugins: () => PLUGINS,
	reload_plugin: ({ id }) => (broadcast("plugin_reloaded", id), null),
	get_profiles: () => [...order],
	set_profile_order: ({ order: o }) => ((order = o), null),
	get_selected_profile: () => getProfile(selected),
	set_selected_profile: ({ id }) => {
		if (!id?.trim()) throw new Error("profile id must not be empty");
		getProfile(id);
		propagateReplicated(selected);
		selected = id;
		broadcast("switch_profile", { device: DEVICE.id, profile: id });
		rerender();
		return null;
	},
	delete_profile: ({ profile }) => {
		if (profile == selected) throw new Error("cannot delete the selected profile; switch to another profile first");
		profiles.delete(profile);
		order = order.filter((p) => p != profile);
		return null;
	},
	rename_profile: ({ oldId, newId, retain }) => {
		const p = structuredClone(getProfile(oldId));
		p.id = newId;
		for (const inst of [...p.keys, ...p.sliders]) if (inst) inst.context = inst.context.replace(`.${oldId}.`, `.${newId}.`);
		profiles.set(newId, p);
		const i = order.indexOf(oldId);
		order = order.filter((x) => x != newId);
		if (retain) order.splice(i + 1, 0, newId);
		else {
			profiles.delete(oldId);
			order[order.indexOf(oldId)] = newId;
			if (selected == oldId) selected = newId;
		}
		return null;
	},
	get_replicated_slots: () => replicated,
	set_slot_replicated: ({ context, replicated: on }) => {
		const key = `${context.controller}.${context.position}`;
		if (on) {
			if (!slotArray(getProfile(context.profile), context.controller)[context.position]) throw new Error("only a slot with an action can be replicated");
			if (!replicated.includes(key)) replicated.push(key);
			propagateReplicated(context.profile);
		} else replicated = replicated.filter((k) => k != key);
		broadcast("replicated_slots", [DEVICE.id, replicated]);
		return replicated;
	},
	create_instance: ({ action: a, context }) => {
		if (!a.controllers.includes(context.controller)) return null;
		const slots = slotArray(getProfile(context.profile), context.controller);
		const parent = slots[context.position];
		if (parent?.children) {
			const child = instance(a, context);
			child.context = ctx(context, (parent.children.at(-1) ? parseCtx(parent.children.at(-1).context).index : 0) + 1);
			parent.children.push(child);
		} else slots[context.position] = instance(a, context);
		return slots[context.position];
	},
	move_instance: ({ source, destination, retain }) => {
		const from = slotArray(getProfile(source.profile), source.controller);
		const to = slotArray(getProfile(destination.profile), destination.controller);
		if (!from[source.position] || to[destination.position]) throw new Error("the destination is not empty");
		const inst = structuredClone(from[source.position]);
		inst.context = ctx(destination);
		to[destination.position] = inst;
		if (!retain) from[source.position] = null;
		return inst;
	},
	remove_instance: ({ context }) => {
		const c = parseCtx(context);
		const slots = slotArray(getProfile(c.profile), c.controller);
		const inst = slots[c.position];
		if (!inst) return null;
		if (inst.context == context) slots[c.position] = null;
		else inst.children = inst.children.filter((ch) => ch.context != context);
		return null;
	},
	set_state: ({ context, index, state: s }) => {
		const c = parseCtx(context);
		const inst = slotArray(getProfile(c.profile), c.controller)[c.position];
		inst.states[index] = s;
		return null;
	},
	trigger_virtual_press: ({ context }) => {
		broadcast("key_moved", { context, pressed: true });
		setTimeout(() => broadcast("key_moved", { context, pressed: false }), 150);
		return null;
	},
	make_info: ({ plugin }) => ({
		application: { font: "", language: "en", platform: "linux", platformVersion: "", version: "2.14.2" },
		plugin: { uuid: plugin, version: "0.1.0" },
		colors: {},
		devicePixelRatio: 1,
		devices: [],
	}),
	switch_property_inspector: () => null,
	get_plugin_global_settings: ({ plugin }) => globalSettings[plugin] ?? {},
	set_plugin_global_settings: ({ plugin, settings: s }) => ((globalSettings[plugin] = s), null),
	get_settings: () => settings,
	set_settings: ({ settings: s }) => ((settings = s), null),
	get_rendered_images: () => images,
	get_remote_info: () => ({ api: 1, version: "2.14.2 (mock)", port_base: 57116 }),
};
const MUTATING = new Set(["create_instance", "move_instance", "remove_instance", "set_state", "set_slot_replicated", "set_device_layout"]);

// ---- WebSocket, just enough for text frames ----

const clients = new Set();
function frame(text) {
	const data = Buffer.from(text);
	const header =
		data.length < 126
			? Buffer.from([0x81, data.length])
			: data.length < 65536
				? Buffer.from([0x81, 126, data.length >> 8, data.length & 255])
				: (() => {
						const h = Buffer.alloc(10);
						h[0] = 0x81;
						h[1] = 127;
						h.writeBigUInt64BE(BigInt(data.length), 2);
						return h;
					})();
	return Buffer.concat([header, data]);
}
function broadcast(event, payload) {
	const text = JSON.stringify({ event, payload });
	for (const socket of clients) socket.write(frame(text));
}
function upgrade(req, socket, onText) {
	const accept = createHash("sha1")
		.update(req.headers["sec-websocket-key"] + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")
		.digest("base64");
	socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
	let buffer = Buffer.alloc(0);
	socket.on("data", (chunk) => {
		buffer = Buffer.concat([buffer, chunk]);
		while (buffer.length >= 2) {
			const opcode = buffer[0] & 15;
			let length = buffer[1] & 127,
				offset = 2;
			if (length == 126) ((length = buffer.readUInt16BE(2)), (offset = 4));
			else if (length == 127) ((length = Number(buffer.readBigUInt64BE(2))), (offset = 10));
			const masked = buffer[1] & 128;
			const total = offset + (masked ? 4 : 0) + length;
			if (buffer.length < total) return;
			const mask = masked ? buffer.subarray(offset, offset + 4) : null;
			const data = Buffer.from(buffer.subarray(total - length, total));
			if (mask) for (let i = 0; i < data.length; i++) data[i] ^= mask[i % 4];
			buffer = buffer.subarray(total);
			if (opcode == 8) return socket.end();
			if (opcode == 1) onText?.(data.toString());
		}
	});
	socket.on("error", () => {});
}

// ---- HTTP ----

const SHIM = `<script>
(() => {
	const NativeWebSocket = window.WebSocket;
	const bridge = (location.protocol === "https:" ? "wss://" : "ws://") + location.host + "/pi-socket";
	function RemoteWebSocket(url, protocols) {
		try { const parsed = new URL(url, location.href); if (parsed.hostname === "127.0.0.1" || parsed.hostname === "localhost") url = bridge; } catch {}
		return protocols === undefined ? new NativeWebSocket(url) : new NativeWebSocket(url, protocols);
	}
	RemoteWebSocket.prototype = NativeWebSocket.prototype;
	window.WebSocket = RemoteWebSocket;
})();
</script>`;
// The part of OpenDeck's webserver injection that matters here: connecting when the parent sends "connect".
const OPENDECK_INJECTION = `<script>window.addEventListener("message", (event) => { if (event.data.event == "connect") connectElgatoStreamDeckSocket(...event.data.payload); });</script>`;

function authorised(req, url) {
	const auth = req.headers.authorization;
	if (auth?.startsWith("Bearer ") && auth.slice(7) == TOKEN) return true;
	const cookie = (req.headers.cookie ?? "")
		.split(";")
		.map((c) => c.trim())
		.find((c) => c.startsWith("od_remote_token="));
	if (cookie && decodeURIComponent(cookie.slice(16)) == TOKEN) return true;
	return url.searchParams.get("token") == TOKEN;
}

const MIME = {
	".html": "text/html; charset=utf-8",
	".js": "text/javascript",
	".css": "text/css",
	".svg": "image/svg+xml",
	".png": "image/png",
	".webmanifest": "application/manifest+json",
	".json": "application/json",
};

const server = createServer(async (req, res) => {
	const url = new URL(req.url, "http://localhost");
	const send = (status, body, type = "application/json") => {
		res.writeHead(status, { "Content-Type": type, "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "authorization, content-type" });
		res.end(typeof body == "string" || Buffer.isBuffer(body) ? body : JSON.stringify(body));
	};
	if (req.method == "OPTIONS") return send(204, "");
	if (url.pathname == "/api/info") return send(200, { name: "OpenDeck", version: "2.14.2 (mock)", api: 1, authorised: authorised(req, url) });
	if (url.pathname == "/api/rpc") {
		if (!authorised(req, url)) return send(401, { error: "unauthorised" });
		let body = "";
		for await (const chunk of req) body += chunk;
		try {
			const { cmd, args = {} } = JSON.parse(body);
			const fn = commands[cmd];
			if (!fn) return send(422, { error: `unknown or unsupported command \`${cmd}\`` });
			const ok = fn(args) ?? null;
			if (MUTATING.has(cmd)) setTimeout(() => (rerender(), broadcast("profile_changed", { cmd })), 60);
			return send(200, { ok });
		} catch (error) {
			return send(422, { error: String(error.message ?? error) });
		}
	}
	if (url.pathname.startsWith("/pi/")) {
		if (!authorised(req, url)) return send(401, "unauthorised", "text/plain");
		const path = decodeURIComponent(url.pathname.slice(3));
		if (path.endsWith("|opendeck_property_inspector")) return send(200, SHIM + readFileSync(join(here, "pi.html"), "utf8") + OPENDECK_INJECTION, MIME[".html"]);
		return send(404, "not found", "text/plain");
	}
	if (url.pathname.startsWith("/opendeck/")) return send(200, readFileSync(join(here, "..", "..", "static", url.pathname.slice(10))), "image/png");
	let file = join(DIST, url.pathname == "/" ? "index.html" : url.pathname);
	if (!file.startsWith(DIST) || !existsSync(file)) file = join(DIST, "index.html");
	if (!existsSync(file)) return send(404, "Build the app first: npx vite build remote-ui", "text/plain");
	send(200, readFileSync(file), MIME[extname(file)] ?? "application/octet-stream");
});

server.on("upgrade", (req, socket) => {
	const url = new URL(req.url, "http://localhost");
	if (!authorised(req, url)) return socket.end("HTTP/1.1 401 Unauthorized\r\n\r\n");
	if (url.pathname == "/api/events") {
		upgrade(req, socket);
		clients.add(socket);
		socket.on("close", () => clients.delete(socket));
	} else if (url.pathname == "/pi-socket") {
		// Emulates the plugin WebSocket server for the property inspector.
		upgrade(req, socket, (text) => {
			const message = JSON.parse(text);
			if (message.event == "getGlobalSettings")
				socket.write(frame(JSON.stringify({ event: "didReceiveGlobalSettings", payload: { settings: globalSettings[HA] } })));
			if (message.event == "setGlobalSettings") globalSettings[HA] = message.payload;
		});
	} else socket.end("HTTP/1.1 404 Not Found\r\n\r\n");
});

rerender();
server.listen(PORT, "0.0.0.0", () => console.log(`Mock OpenDeck remote on http://127.0.0.1:${PORT}/#token=${TOKEN}`));
