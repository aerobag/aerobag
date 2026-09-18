// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

// Serialized into the browser. Subscribe before CDP sends input, then await the
// browser's event, not a CDP acknowledgement or an arbitrary number of frames.
export function beginBrowserInput({ selector, point, bounds, eventTypes, timeoutMs }) {
  const surface = document.querySelector(selector);
  if (!surface) throw new Error(`input surface is missing: ${selector}`);
  const rect = surface.getBoundingClientRect();
  if (bounds && ["left", "top", "width", "height"].some((key) => Math.abs(rect[key] - bounds[key]) > 1)) {
    throw new Error(`input surface moved after readiness: ${selector}`);
  }
  const hit = document.elementFromPoint(point.x, point.y);
  const control = hit?.closest('button,input,textarea,select,[role="button"],.trayScrim');
  if (!hit || !surface.contains(hit) || (control && control !== surface && surface.contains(control))) {
    throw new Error(`input surface is obstructed: ${selector}; hit ${hit?.outerHTML?.slice(0, 300) ?? "nothing"}`);
  }
  if (window.__aerobagInputReceipt) throw new Error("previous browser input receipt was not disposed");
  const events = [];
  let resolve;
  const completed = new Promise((done) => { resolve = done; });
  const finish = (error = null) => {
    cleanup();
    resolve({ events, error });
  };
  const listener = (event) => {
    if (!eventTypes.includes(event.type)) return;
    const delivered = event.composedPath().includes(surface);
    events.push({ type: event.type, delivered, target: event.target?.tagName ?? null,
      class_name: String(event.target?.className?.baseVal ?? event.target?.className ?? ""),
      x: event.clientX, y: event.clientY });
    if (!delivered) {
      finish(`input ${event.type} missed ${selector}`);
    } else if (event.type === eventTypes.at(-1)) {
      // React's delegated handler runs during this dispatch. Completion checks
      // still have to prove its semantic effect after this delivery receipt.
      queueMicrotask(() => finish());
    }
  };
  const timer = setTimeout(() => finish(`browser did not deliver ${eventTypes.at(-1)} to ${selector}`), timeoutMs);
  const cleanup = () => {
    clearTimeout(timer);
    for (const type of eventTypes) document.removeEventListener(type, listener, true);
  };
  for (const type of eventTypes) document.addEventListener(type, listener, true);
  window.__aerobagInputReceipt = { completed, cleanup };
}

export async function finishBrowserInput() {
  const receipt = window.__aerobagInputReceipt;
  if (!receipt) throw new Error("browser input receipt is missing");
  try {
    const result = await receipt.completed;
    if (result.error) throw new Error(`${result.error}; events=${JSON.stringify(result.events)}`);
    return result;
  } finally {
    receipt.cleanup();
    delete window.__aerobagInputReceipt;
  }
}

export function cancelBrowserInput() {
  window.__aerobagInputReceipt?.cleanup();
  delete window.__aerobagInputReceipt;
}
