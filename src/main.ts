import { createApp } from "vue";
import App from "./App.vue";
import "./styles/main.css";

console.log('Creating Vue app...');

window.onerror = (msg, url, line, col, error) => {
  console.error('Global error:', msg, url, line, col, error);
};

window.onunhandledrejection = (event) => {
  console.error('Unhandled promise rejection:', event.reason);
};

const app = createApp(App);
console.log('Vue app created, mounting...');
app.mount("#app");
console.log('App mounted');
