import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { initNetwork } from './lib/stores/network';

// LAN shop network store: subscribes to backend status/event pushes. Client
// forwarding is handled entirely in Rust (invoke_handler wrapper in lib.rs).
initNetwork();

const app = mount(App, {
  target: document.getElementById('app')!,
});

// Global: scrolling must never alter a focused number input. When the wheel
// moves while a number input has focus, drop the focus first — Chromium then
// applies the scroll to the modal/page instead of changing the value. Covers
// every price/qty input app-wide without per-input handlers.
document.addEventListener(
  'wheel',
  () => {
    const el = document.activeElement as HTMLInputElement | null;
    if (el && el.type === 'number') {
      el.blur();
    }
  },
  { passive: true, capture: true }
);

export default app;