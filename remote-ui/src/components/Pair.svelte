<script lang="ts">
	import { setToken, serverInfo } from "../api.ts";
	import { createEventDispatcher } from "svelte";

	const dispatch = createEventDispatcher<{ paired: void }>();
	let value = "";
	let error = "";
	let busy = false;

	function extractToken(text: string): string {
		text = text.trim();
		const match = text.match(/[#?&]token=([A-Za-z0-9]+)/);
		return match ? match[1] : text;
	}

	async function pair() {
		error = "";
		const token = extractToken(value);
		if (!token) return;
		busy = true;
		setToken(token);
		try {
			const info = await serverInfo();
			if (info.authorised) dispatch("paired");
			else {
				setToken(null);
				error = "That code was not accepted. Check it and try again.";
			}
		} catch {
			error = "Could not reach the deck. Make sure your phone is on the same network.";
		} finally {
			busy = false;
		}
	}
</script>

<main>
	<img src="./icon.svg" alt="" width="72" height="72" />
	<h1>Pair with your deck</h1>
	<p class="muted">
		Enter the pairing token from <code>remote.json</code> in the OpenDeck config folder on the deck, or open the pairing link that OpenDeck prints when it starts.
	</p>
	<form on:submit|preventDefault={pair}>
		<label class="field">
			<span>Pairing token or link</span>
			<input class="input" bind:value autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="Paste token or link" />
		</label>
		{#if error}<p class="error">{error}</p>{/if}
		<button class="btn primary" type="submit" disabled={busy || !value.trim()}>{busy ? "Pairing…" : "Pair"}</button>
	</form>
</main>

<style>
	main {
		max-width: 420px;
		margin: 0 auto;
		padding: calc(48px + var(--safe-top)) 20px 40px;
		text-align: center;
	}
	h1 {
		font-size: 24px;
		margin: 16px 0 8px;
	}
	form {
		text-align: left;
		margin-top: 24px;
	}
	form .btn {
		width: 100%;
	}
	code {
		background: var(--surface-2);
		padding: 1px 5px;
		border-radius: 5px;
		font-size: 14px;
	}
	.error {
		color: var(--danger);
		font-size: 14px;
	}
</style>
