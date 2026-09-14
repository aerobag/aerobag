// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

/** Owns the browser subscription, including callbacks queued before a pause. */
export class BrowserGeolocationWatch {
  private id: number | null = null;
  private generation = 0;
  private disposed = false;
  constructor(private readonly geo: Pick<Geolocation, "watchPosition" | "clearWatch">,
    private readonly position: PositionCallback, private readonly error: PositionErrorCallback,
    private readonly started: () => void) {}

  setPaused(paused: boolean) {
    if (paused || this.disposed) { this.stop(); return; }
    if (this.id !== null) return;
    const generation = ++this.generation;
    this.id = this.geo.watchPosition(
      value => { if (this.generation === generation) this.position(value); },
      error => { if (this.generation === generation) this.error(error); },
      { enableHighAccuracy: true, maximumAge: 1_000, timeout: 15_000 });
    this.started();
  }
  private stop() {
    ++this.generation;
    if (this.id !== null) this.geo.clearWatch(this.id);
    this.id = null;
  }
  dispose() { this.disposed = true; this.stop(); }
}
