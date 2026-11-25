// Client for OpenDeck's remote API (src-tauri/src/remote). Commands and argument names match the desktop frontend's `invoke` calls.

const TOKEN_KEY = "opendeck-remote-token";

function readStorage(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

function writeStorage(key: string, value: string | null) {
	try {
		if (value == null) localStorage.removeItem(key);
		else localStorage.setItem(key, value);
	} catch {}
}

let token: string | null = readStorage(TOKEN_KEY);

/// Store the pairing token, and set it as a cookie so that property inspector frames and their assets are authorised too.
export function setToken(value: string | null) {
	token = value;
	writeStorage(TOKEN_KEY, value);
	const secure = location.protocol == "https:" ? "; Secure" : "";
	if (value) document.cookie = `od_remote_token=${encodeURIComponent(value)}; path=/; max-age=31536000; SameSite=Strict${secure}`;
	else document.cookie = "od_remote_token=; path=/; max-age=0";
}

export function getToken(): string | null {
	return token;
}

// Refresh the cookie on every start, in case the browser dropped it.
if (token) setToken(token);

export class ApiError extends Error {
	status: number;
	constructor(message: string, status: number) {
		super(message);
		this.status = status;
	}
}

export type ServerInfo = { name: string; version: string; api: number; authorised: boolean };

export async function serverInfo(): Promise<ServerInfo> {
	const response = await fetch("./api/info", { headers: token ? { Authorization: `Bearer ${token}` } : {} });
	if (!response.ok) throw new ApiError(`Server answered ${response.status}`, response.status);
	return await response.json();
}

export async function invoke<T = any>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
	const response = await fetch("./api/rpc", {
		method: "POST",
		headers: { "Content-Type": "application/json", ...(token ? { Authorization: `Bearer ${token}` } : {}) },
		body: JSON.stringify({ cmd, args }),
	});
	let body: any = null;
	try {
		body = await response.json();
	} catch {}
	if (!response.ok || (body && "error" in body)) {
		throw new ApiError(body?.error ?? `Request failed with ${response.status}`, response.status);
	}
	return body?.ok as T;
}

export type DeckEvent = { event: string; payload: any };
type Handler = (event: DeckEvent) => void;

/// A WebSocket to `/api/events` that reconnects with backoff, and reports whether it is connected.
export class EventStream {
	private socket: WebSocket | null = null;
	private delay = 500;
	private closed = false;
	private timer: ReturnType<typeof setTimeout> | null = null;

	constructor(
		private onEvent: Handler,
		private onStatus: (connected: boolean) => void,
	) {
		this.connect();
	}

	private connect() {
		if (this.closed) return;
		const url = new URL("./api/events", location.href);
		url.protocol = location.protocol == "https:" ? "wss:" : "ws:";
		if (token) url.searchParams.set("token", token);
		const socket = new WebSocket(url);
		this.socket = socket;
		socket.onopen = () => {
			this.delay = 500;
			this.onStatus(true);
			// Anything may have changed while disconnected.
			this.onEvent({ event: "resync", payload: null });
		};
		socket.onmessage = (message) => {
			try {
				this.onEvent(JSON.parse(message.data));
			} catch {}
		};
		socket.onclose = () => {
			this.onStatus(false);
			if (this.closed) return;
			this.timer = setTimeout(() => this.connect(), this.delay);
			this.delay = Math.min(this.delay * 2, 10_000);
		};
	}

	close() {
		this.closed = true;
		if (this.timer) clearTimeout(this.timer);
		this.socket?.close();
	}
}

/// URL for an image path as used in action states and manifests.
export function imageUrl(image: string | null | undefined): string {
	if (!image) return "./alert.svg";
	if (image.startsWith("data:")) return image;
	if (image.startsWith("opendeck/")) return "./" + image;
	if (image.startsWith("http://") || image.startsWith("https://")) return image;
	return "./pi/" + encodeURI(image).replace(/#/g, "%23").replace(/\?/g, "%3F");
}

/// URL that loads a property inspector through the remote server.
export function propertyInspectorUrl(path: string): string {
	return imageUrl(path) + encodeURIComponent("|opendeck_property_inspector");
}
