// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export class CoalescedAsyncRunner {
  private requested = false;
  private running: Promise<void> | null = null;

  constructor(private readonly work: () => Promise<void>) {}

  request(): Promise<void> {
    this.requested = true;
    if (this.running) return this.running;

    const run = this.drainRequests();
    this.running = run;
    void run.then(
      () => this.finish(run),
      () => this.finish(run),
    );
    return run;
  }

  private async drainRequests(): Promise<void> {
    while (this.requested) {
      this.requested = false;
      await this.work();
    }
  }

  private finish(run: Promise<void>): void {
    if (this.running !== run) return;
    this.running = null;
    if (this.requested) void this.request();
  }
}
