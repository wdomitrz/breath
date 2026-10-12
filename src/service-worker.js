// Browser lifecycle/cache plumbing only; all pacer/app logic remains Rust.
// No skipWaiting: an update takes over after old app tabs close, avoiding
// mixed-version wasm/bindings while someone is mid-breath.
// The fetch handler answers only for URLs inside this app's own directory,
// checked per request rather than assumed from the scope: a scope is a
// registration's claim, not a promise, and these apps share one origin with
// pages that are not this app.
const ROOT = new URL('./', self.location.href);
const CACHE = 'breath-' + ROOT.pathname + '-__VERSION__';
const ASSETS = ['./', 'app.js', 'app_bg.wasm', 'manifest.webmanifest', 'icon-192.png', 'icon-512.png', 'icon.svg', 'index.html'].map(p => new URL(p, ROOT).href);
const IS_OWN = url => url.startsWith(ROOT.href);
self.addEventListener('install', event => {
  event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(ASSETS)));
});
self.addEventListener('activate', event => {
  event.waitUntil((async () => {
    const prefix = 'breath-' + ROOT.pathname + '-';
    for (const key of await caches.keys()) {
      if (key.startsWith(prefix) && key !== CACHE) await caches.delete(key);
    }
    await self.clients.claim();
  })());
});
self.addEventListener('fetch', event => {
  // Only this app's precached URLs, inside its own directory, are answered;
  // every other request is left alone whatever the scope says.
  const url = event.request.url;
  if (event.request.method !== 'GET' || !IS_OWN(url) || !ASSETS.includes(url)) return;
  event.respondWith((async () => {
    const cache = await caches.open(CACHE);
    const cached = await cache.match(event.request);
    return cached || fetch(event.request);
  })());
});
