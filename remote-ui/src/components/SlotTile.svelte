<script lang="ts">
	import type { ActionInstance, Controller } from "../state.ts";
	import { imageUrl } from "../api.ts";

	export let controller: Controller;
	export let instance: ActionInstance | null;
	/// The image the deck is showing for this key, if the desktop frontend has rendered one.
	export let rendered: string | undefined = undefined;
	export let replicated = false;
	export let pressed = false;
	export let selected = false;
	export let label: string;

	$: state = instance ? instance.states[instance.current_state] : null;
	$: encoderIcon = instance?.action.encoder?.icon || state?.image || instance?.action.icon;
	$: title = state ? (state.text_override ?? state.text) : "";
	$: showTitle = !!state?.show && !!title;
</script>

<button
	class="tile {controller == 'Encoder' ? 'encoder' : 'key'}"
	class:empty={!instance}
	class:pressed
	class:selected
	aria-label={instance ? `${label}: ${instance.action.name}${title ? `, ${title}` : ""}` : `${label}: empty`}
	on:click
>
	{#if controller == "Encoder"}
		<span class="knob">
			{#if instance}
				<img src={imageUrl(encoderIcon)} alt="" draggable="false" />
			{:else}
				<span class="plus">+</span>
			{/if}
		</span>
		<span class="caption">{instance ? title || instance.action.name : ""}</span>
	{:else if rendered}
		<img class="face" src={rendered} alt="" draggable="false" />
	{:else if instance && state}
		<span class="face fallback" style:background-color={state.background_colour || "#000"}>
			<img src={imageUrl(state.image_override ?? state.image ?? instance.action.icon)} alt="" draggable="false" />
			{#if showTitle}
				<span class="title {state.alignment}" style:color={state.colour}>{title}</span>
			{/if}
		</span>
	{:else}
		<span class="face blank"><span class="plus">+</span></span>
	{/if}
	{#if replicated}<span class="badge" title="On every page">⇄</span>{/if}
</button>

<style>
	.tile {
		position: relative;
		width: 100%;
		height: 100%;
		padding: 0;
		border: none;
		background: none;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		touch-action: manipulation;
	}
	.face {
		display: block;
		width: 100%;
		aspect-ratio: 1;
		border-radius: 14%;
		overflow: hidden;
		background: #000;
		border: 2px solid #2a2a2a;
		object-fit: cover;
		transition: transform 0.08s;
	}
	.fallback {
		position: relative;
	}
	.fallback img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.title {
		position: absolute;
		left: 4%;
		right: 4%;
		text-align: center;
		font-weight: 700;
		font-size: clamp(9px, 3.2vw, 15px);
		line-height: 1.1;
		text-shadow:
			0 0 3px #000,
			0 0 2px #000;
		overflow-wrap: anywhere;
	}
	.title.top {
		top: 6%;
	}
	.title.middle {
		top: 50%;
		transform: translateY(-50%);
	}
	.title.bottom {
		bottom: 6%;
	}
	.blank {
		display: flex;
		align-items: center;
		justify-content: center;
		background: #121212;
		border-style: dashed;
		border-color: #3a3a3a;
	}
	.plus {
		color: #5a5a5a;
		font-size: clamp(18px, 7vw, 32px);
		font-weight: 300;
	}
	.knob {
		width: 78%;
		aspect-ratio: 1;
		border-radius: 50%;
		background: radial-gradient(circle at 35% 30%, #3a3a3a, #161616 70%);
		border: 2px solid #2f2f2f;
		box-shadow: 0 4px 10px rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		transition: transform 0.08s;
	}
	.knob img {
		width: 58%;
		height: 58%;
		object-fit: contain;
		border-radius: 20%;
	}
	.encoder.empty .knob {
		border-style: dashed;
		border-color: #3a3a3a;
	}
	.caption {
		margin-top: 4px;
		max-width: 100%;
		font-size: 11px;
		color: var(--muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-height: 15px;
	}
	.pressed .face,
	.pressed .knob,
	.tile:active .face,
	.tile:active .knob {
		transform: scale(0.94);
	}
	.selected .face,
	.selected .knob {
		border-color: var(--accent);
		box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.35);
	}
	.badge {
		position: absolute;
		top: -4px;
		right: -4px;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: var(--accent);
		color: #fff;
		font-size: 12px;
		display: flex;
		align-items: center;
		justify-content: center;
		border: 2px solid var(--bg);
	}
</style>
