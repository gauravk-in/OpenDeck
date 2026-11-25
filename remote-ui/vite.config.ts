import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The mobile web app served by OpenDeck's remote server (src-tauri/src/remote).
// During development, `OPENDECK_REMOTE=http://<host>:57200 vite remote-ui` proxies API calls to a running OpenDeck or the mock server.
const target = process.env.OPENDECK_REMOTE ?? "http://127.0.0.1:57200";

export default defineConfig({
	root: __dirname,
	base: "./",
	plugins: [svelte({ configFile: `${__dirname}/svelte.config.js` })],
	build: {
		outDir: "dist",
		emptyOutDir: true,
		target: "es2020",
	},
	server: {
		proxy: {
			"/api/events": { target, ws: true },
			"/pi-socket": { target, ws: true },
			"/api": target,
			"/pi": target,
			"/opendeck": target,
		},
	},
});
