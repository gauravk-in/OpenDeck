<script lang="ts">
	// Hosts a plugin's property inspector, doing what the desktop's PropertyInspectorView does: send the connect message,
	// tell OpenDeck it appeared, and serve the requests the injected OpenDeck script sends to its parent window.
	import type { ActionInstance, DeviceInfo } from "../state.ts";
	import { invoke, propertyInspectorUrl } from "../api.ts";
	import { onDestroy, onMount } from "svelte";

	export let instance: ActionInstance;
	export let device: DeviceInfo;

	let iframe: HTMLIFrameElement;
	let popup = false;
	let height = 320;
	let resizeObserver: ResizeObserver | null = null;

	const context = instance.context;
	const src = propertyInspectorUrl(instance.action.property_inspector);

	async function onLoad() {
		const split = context.split(".");
		const position = parseInt(split[split.length - 2]);
		const controller = split[split.length - 3];
		const coordinates =
			controller == "Encoder" ? { row: 0, column: position } : { row: Math.floor(position / device.columns), column: position % device.columns };
		let port = 57116;
		try {
			port = await invoke<number>("get_port_base");
		} catch {}
		const info = JSON.stringify(await invoke("make_info", { plugin: instance.action.plugin }));
		iframe.contentWindow?.postMessage(
			{
				event: "connect",
				payload: [
					port,
					context,
					"registerPropertyInspector",
					info,
					JSON.stringify({
						action: instance.action.uuid,
						context,
						device: device.id,
						payload: {
							settings: instance.settings,
							coordinates,
							controller,
							state: instance.current_state,
							isInMultiAction: parseInt(split[split.length - 1]) != 0,
						},
					}),
				],
			},
			location.origin,
		);
		// The frame is same-origin, so it can be sized to its content.
		try {
			const body = iframe.contentDocument?.body;
			if (body) {
				const fit = () => (height = Math.min(Math.max(body.scrollHeight + 16, 160), 900));
				fit();
				resizeObserver = new ResizeObserver(fit);
				resizeObserver.observe(body);
			}
		} catch {}
	}

	async function onMessage({ data, source }: MessageEvent) {
		// Messages come from the inspector itself, or from a window it opened (which names the inspector in its payload).
		if (!iframe || (source != iframe.contentWindow && data?.payload != context)) return;
		if (data?.event == "windowOpened") popup = true;
		else if (data?.event == "windowClosed") popup = false;
		else if (data?.event == "openUrl") window.open(data.payload, "_blank", "noopener");
		else if (data?.event == "fetch" && data.payload?.context == context) {
			try {
				const response = await fetch(...(data.payload.args as [RequestInfo, RequestInit?]));
				const body = new Uint8Array(await response.arrayBuffer());
				iframe.contentWindow?.postMessage(
					{
						event: "fetchResponse",
						payload: {
							id: data.payload.id,
							response: { url: response.url, body, headers: [...response.headers.entries()], status: response.status, statusText: response.statusText },
						},
					},
					location.origin,
				);
			} catch (error) {
				iframe.contentWindow?.postMessage({ event: "fetchError", payload: { id: data.payload.id, error: String(error) } }, location.origin);
			}
		}
	}

	function closePopup() {
		popup = false;
		iframe.contentWindow?.postMessage({ event: "windowClosed" }, location.origin);
	}

	onMount(() => {
		window.addEventListener("message", onMessage);
		invoke("switch_property_inspector", { old: null, new: context }).catch(() => {});
	});
	onDestroy(() => {
		window.removeEventListener("message", onMessage);
		resizeObserver?.disconnect();
		invoke("switch_property_inspector", { old: context, new: null }).catch(() => {});
	});
</script>

<div class="frame" class:popup>
	{#if popup}<button class="btn close" on:click={closePopup}>Done</button>{/if}
	<iframe bind:this={iframe} title="Action settings" name={context} {src} on:load={onLoad} style:height={popup ? "100%" : `${height}px`} />
</div>

<style>
	.frame {
		border: 1px solid var(--border);
		border-radius: var(--radius);
		overflow: hidden;
		background: #2d2d2d;
	}
	iframe {
		display: block;
		width: 100%;
		border: none;
		background: #2d2d2d;
	}
	.popup {
		position: fixed;
		inset: 0;
		z-index: 80;
		border-radius: 0;
		padding-top: calc(52px + var(--safe-top));
	}
	.close {
		position: absolute;
		top: calc(6px + var(--safe-top));
		right: 8px;
	}
</style>
