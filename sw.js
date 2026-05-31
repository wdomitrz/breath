const CACHE_NAME = "breath-pwa-v1";
const FILES = [
  "./",
  "./index.html",
  "./style.css",
  "./app.js",
  "./sw.js",
  "./icon.svg",
  "./manifest.json",
];
const IS_LOCAL = ["localhost", "127.0.0.1", "::1"].includes(
  self.location.hostname,
);

self.addEventListener("install", (event) => {
  if (!IS_LOCAL) {
    event.waitUntil(
      caches.open(CACHE_NAME).then((cache) => cache.addAll(FILES)),
    );
  }

  self.skipWaiting();
});

self.addEventListener("fetch", (event) => {
  if (IS_LOCAL || event.request.method !== "GET") {
    return;
  }

  event.respondWith(
    caches
      .match(event.request)
      .then((response) => response || fetch(event.request)),
  );
});
