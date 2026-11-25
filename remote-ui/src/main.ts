import "./app.css";
import App from "./App.svelte";

const app = new App({ target: document.getElementById("app")! });

if ("serviceWorker" in navigator && location.protocol != "file:") {
	navigator.serviceWorker.register("./sw.js").catch(() => {});
}

export default app;
