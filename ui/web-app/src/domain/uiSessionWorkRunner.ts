// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  UiSessionWorkCompletionDecision as CompletionDecision,
  UiSessionWorkKind,
  UiSessionWorkRequest as WorkRequest,
  UiSessionWorkRequestDecision as RequestDecision,
} from "../generated/sessionWorkWire";

export type { UiSessionWorkKind } from "../generated/sessionWorkWire";

type Awaitable<T> = Promise<T> | T;

export type UiSessionWorkSchedulerBridge = {
  create(): Awaitable<number>;
  request(handle: number, requestJson: string): Awaitable<string>;
  complete(handle: number, requestId: number): Awaitable<string>;
  destroy(handle: number): Awaitable<void>;
};

type WorkOutcome<T> =
  | { ok: true; value: T }
  | { ok: false; error: unknown };

type WorkPayload<T> = {
  operation: () => Promise<T>;
  resolve(value: T): void;
  reject(error: unknown): void;
};

const MAX_REQUEST_ID = 0xffff_ffff;

export class UiSessionWorkCancelledError extends Error {
  constructor(readonly reason: string) {
    super(`UI session work cancelled: ${reason}`);
    this.name = "UiSessionWorkCancelledError";
  }
}

export class WebUiSessionWorkRunner {
  private readonly payloads = new Map<number, WorkPayload<unknown>>();
  private nextRequestId = 1;
  private schedulerMutation: Promise<void> = Promise.resolve();
  private closePromise: Promise<void> | null = null;
  private closed = false;

  private constructor(
    private readonly bridge: UiSessionWorkSchedulerBridge,
    private readonly schedulerHandle: number,
  ) {}

  static async create(bridge: UiSessionWorkSchedulerBridge): Promise<WebUiSessionWorkRunner> {
    return new WebUiSessionWorkRunner(bridge, await bridge.create());
  }

  run<T>(
    kind: UiSessionWorkKind,
    coalesceKey: string | null,
    operation: () => Promise<T> | T,
  ): Promise<T> {
    if (this.closed) {
      return Promise.reject(new UiSessionWorkCancelledError("runner_closed"));
    }
    if (this.nextRequestId > MAX_REQUEST_ID) {
      return Promise.reject(new Error("UI session work request IDs exhausted"));
    }
    const request: WorkRequest = {
      id: this.nextRequestId++,
      kind,
      coalesce_key: coalesceKey,
      requested_at_ms: Math.max(0, Math.round(performance.now())),
    };
    return new Promise<T>((resolve, reject) => {
      this.payloads.set(request.id, {
        operation: async () => operation(),
        resolve: (value) => resolve(value as T),
        reject,
      });
      void this.mutateScheduler(() =>
        this.bridge.request(this.schedulerHandle, JSON.stringify(request)),
      ).then(
        (decisionJson) => {
          try {
            this.applyRequestDecision(parseRequestDecision(decisionJson));
          } catch (error) {
            this.failScheduler(error);
          }
        },
        (error) => this.failScheduler(error),
      );
    });
  }

  close(): Promise<void> {
    if (this.closePromise) {
      return this.closePromise;
    }
    this.closed = true;
    this.rejectAll(new UiSessionWorkCancelledError("runner_closed"));
    this.closePromise = this.mutateScheduler(() =>
      this.bridge.destroy(this.schedulerHandle),
    );
    return this.closePromise;
  }

  private applyRequestDecision(decision: RequestDecision): void {
    if (this.closed) {
      return;
    }
    if (decision.kind === "queued") {
      if (decision.replaced_request_id != null) {
        this.rejectPayload(
          decision.replaced_request_id,
          new UiSessionWorkCancelledError("replaced_by_newer_pending"),
        );
      }
      return;
    }
    this.start(decision.request);
  }

  private start(request: WorkRequest): void {
    if (this.closed) {
      return;
    }
    const payload = this.payloads.get(request.id);
    if (!payload) {
      void this.finish(request.id, null);
      return;
    }
    void payload.operation().then(
      (value) => this.finish(request.id, { ok: true, value }),
      (error) => this.finish(request.id, { ok: false, error }),
    );
  }

  private async finish(
    requestId: number,
    outcome: WorkOutcome<unknown> | null,
  ): Promise<void> {
    if (this.closed) {
      return;
    }
    let completion: CompletionDecision;
    try {
      completion = parseCompletionDecision(await this.mutateScheduler(() =>
        this.bridge.complete(this.schedulerHandle, requestId),
      ));
    } catch (error) {
      this.failScheduler(error);
      return;
    }
    const payload = this.payloads.get(requestId);
    this.payloads.delete(requestId);
    if (!this.closed && payload) {
      if (completion.result_action.kind === "drop") {
        payload.reject(new UiSessionWorkCancelledError(completion.result_action.reason));
      } else if (outcome?.ok) {
        payload.resolve(outcome.value);
      } else if (outcome) {
        payload.reject(outcome.error);
      } else {
        payload.reject(new UiSessionWorkCancelledError("payload_missing"));
      }
    }
    if (!this.closed && completion.next) {
      this.start(completion.next);
    }
  }

  private mutateScheduler<T>(operation: () => Awaitable<T>): Promise<T> {
    const result = this.schedulerMutation.then(operation);
    this.schedulerMutation = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  }

  private failScheduler(error: unknown): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    this.rejectAll(error);
    this.closePromise = this.mutateScheduler(() =>
      this.bridge.destroy(this.schedulerHandle),
    ).catch(() => undefined);
  }

  private rejectPayload(requestId: number, error: unknown): void {
    const payload = this.payloads.get(requestId);
    if (!payload) {
      return;
    }
    this.payloads.delete(requestId);
    payload.reject(error);
  }

  private rejectAll(error: unknown): void {
    for (const payload of this.payloads.values()) {
      payload.reject(error);
    }
    this.payloads.clear();
  }
}

function parseRequestDecision(json: string): RequestDecision {
  const value = JSON.parse(json) as Partial<RequestDecision>;
  if (value.kind === "start") {
    return { kind: "start", request: requireWorkRequest(value.request) };
  }
  if (value.kind === "queued") {
    const replaced = value.replaced_request_id;
    if (replaced !== null && replaced !== undefined && !isRequestId(replaced)) {
      throw new Error(`invalid replaced UI session work request ID: ${String(replaced)}`);
    }
    return { kind: "queued", replaced_request_id: replaced ?? null };
  }
  throw new Error(`unknown UI session work request decision: ${json}`);
}

function parseCompletionDecision(json: string): CompletionDecision {
  const value = JSON.parse(json) as Partial<CompletionDecision>;
  const action = value.result_action;
  if (!action || (action.kind !== "land" && action.kind !== "drop")) {
    throw new Error(`invalid UI session work result action: ${json}`);
  }
  if (action.kind === "drop" && typeof action.reason !== "string") {
    throw new Error(`invalid UI session work drop reason: ${json}`);
  }
  return {
    result_action: action,
    next: value.next == null ? null : requireWorkRequest(value.next),
  };
}

function requireWorkRequest(value: unknown): WorkRequest {
  if (!value || typeof value !== "object") {
    throw new Error("UI session work decision has no request");
  }
  const request = value as Partial<WorkRequest>;
  if (
    !isRequestId(request.id)
    || typeof request.kind !== "string"
    || (request.coalesce_key !== null && typeof request.coalesce_key !== "string")
    || typeof request.requested_at_ms !== "number"
  ) {
    throw new Error(`invalid UI session work request: ${JSON.stringify(value)}`);
  }
  return request as WorkRequest;
}

function isRequestId(value: unknown): value is number {
  return Number.isInteger(value) && Number(value) > 0 && Number(value) <= MAX_REQUEST_ID;
}
