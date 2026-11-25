// Network-first service worker: the app always talks to the live deck, but the shell still opens when the deck is briefly unreachable.
const CACHE = "opendeck-remote-v1";
self.addEventListener("install", (event) => {
	event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(["./", "./index.html", "./manifest.webmanifest", "./icon.svg"])));
	self.skipWaiting();
});
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (event) => {
	const url = new URL(event.request.url);
	if (event.request.method != "GET" || url.origin != location.origin) return;
	if (url.pathname.includes("/api/") || url.pathname.includes("/pi") || url.pathname.includes("/opendeck/")) return;
	event.respondWith(
		fetch(event.request)
			.then((response) => {
				const copy = response.clone();
				caches.open(CACHE).then((cache) => cache.put(event.request, copy));
				return response;
			})
			.catch(() => caches.match(event.request).then((hit) => hit ?? caches.match("./index.html"))),
	);
});
