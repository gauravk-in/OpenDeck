<script lang="ts">
	// A bottom sheet on phones, a centred panel on wider screens.
	import { createEventDispatcher, onMount } from "svelte";

	export let title: string;
	export let back = false;
	const dispatch = createEventDispatcher<{ close: void; back: void }>();

	onMount(() => {
		const onKey = (event: KeyboardEvent) => event.key == "Escape" && dispatch("close");
		window.addEventListener("keydown", onKey);
		document.body.style.overflow = "hidden";
		return () => {
			window.removeEventListener("keydown", onKey);
			document.body.style.overflow = "";
		};
	});
</script>

<div class="backdrop" on:click|self={() => dispatch("close")} role="presentation">
	<div class="sheet" role="dialog" aria-modal="true" aria-label={title}>
		<header>
			{#if back}
				<button class="btn ghost icon" on:click={() => dispatch("back")} aria-label="Back">‹</button>
			{:else}
				<span class="spacer" />
			{/if}
			<h2>{title}</h2>
			<button class="btn ghost icon" on:click={() => dispatch("close")} aria-label="Close">✕</button>
		</header>
		<div class="content">
			<slot />
		</div>
		{#if $$slots.footer}
			<footer><slot name="footer" /></footer>
		{/if}
	</div>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		display: flex;
		align-items: flex-end;
		justify-content: center;
		z-index: 50;
		animation: fade 0.15s ease-out;
	}
	.sheet {
		width: 100%;
		max-width: 560px;
		max-height: calc(100dvh - var(--safe-top) - 24px);
		display: flex;
		flex-direction: column;
		background: var(--surface);
		border: 1px solid var(--border);
		border-bottom: none;
		border-radius: 20px 20px 0 0;
		animation: rise 0.2s ease-out;
	}
	@media (min-width: 700px) {
		.backdrop {
			align-items: center;
		}
		.sheet {
			border-radius: 20px;
			border-bottom: 1px solid var(--border);
			max-height: 85dvh;
		}
	}
	header {
		display: flex;
		align-items: center;
		padding: 6px 6px 0;
	}
	h2 {
		flex: 1;
		margin: 0;
		font-size: 17px;
		text-align: center;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.icon,
	.spacer {
		width: 44px;
		padding: 0;
		font-size: 22px;
		flex: none;
	}
	.content {
		overflow-y: auto;
		padding: 8px 16px 16px;
		-webkit-overflow-scrolling: touch;
	}
	footer {
		display: flex;
		gap: 10px;
		padding: 10px 16px calc(10px + var(--safe-bottom));
		border-top: 1px solid var(--border);
	}
	footer :global(.btn) {
		flex: 1;
	}
	@keyframes rise {
		from {
			transform: translateY(40px);
			opacity: 0.4;
		}
	}
	@keyframes fade {
		from {
			opacity: 0;
		}
	}
</style>
