const message = document.querySelector("#message");
const button = document.querySelector("#hello-button");

button.addEventListener("click", () => {
  message.textContent = "Hello from app.js!";
});

if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("./sw.js").catch((error) => {
    console.warn("Service worker registration failed.", error);
  });
}
