// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, useRef, useState } from "react";
import {
  RASTER_TILE_LOAD_RECOVERY_DELAY_MS,
  RASTER_TILE_LOAD_RETRY_LIMIT,
  rasterTileLoadUrl,
} from "./domain/rasterTileLoadRecovery";

type Props = {
  src: string;
  initialSrc?: string;
  onLoaded: () => void;
  onFailed: () => void;
  onError: (attempt: number) => void;
  onRecovery: (attempt: number, trigger: "error" | "watchdog") => void;
};

type LoadState = {
  attempts: Array<"pending" | "failed">;
  winner: number | null;
};

export function RasterTileImage(props: Props) {
  // A different resource owns a new lifecycle; geometry-only rerenders do not.
  return <LoadingRasterTileImage key={JSON.stringify([props.src, props.initialSrc])} {...props} />;
}

function LoadingRasterTileImage(props: Props) {
  const [state, setState] = useState<LoadState>({ attempts: ["pending"], winner: null });
  const stateRef = useRef(state);
  const active = useRef(true);
  const callbacks = useRef(props);
  useEffect(() => { callbacks.current = props; });

  function publish(next: LoadState) {
    stateRef.current = next;
    setState(next);
  }

  function recover(trigger: "error" | "watchdog") {
    const current = stateRef.current;
    if (!active.current || current.winner !== null || current.attempts.length > RASTER_TILE_LOAD_RETRY_LIMIT) return;
    const attempt = current.attempts.length;
    publish({ ...current, attempts: [...current.attempts, "pending"] });
    callbacks.current.onRecovery(attempt, trigger);
  }

  useEffect(() => {
    active.current = true;
    // A single parallel probe can recover a stuck browser request. Keep the
    // original alive: elapsed time is not evidence of a failed transfer.
    const timer = window.setTimeout(() => recover("watchdog"), RASTER_TILE_LOAD_RECOVERY_DELAY_MS);
    return () => {
      active.current = false;
      window.clearTimeout(timer);
    };
  }, []);

  function complete(attempt: number, succeeded: boolean) {
    const current = stateRef.current;
    if (!active.current || current.winner !== null || current.attempts[attempt] !== "pending") return;
    if (succeeded) {
      publish({ ...current, winner: attempt });
      callbacks.current.onLoaded();
      return;
    }
    const attempts = current.attempts.map((status, index) => index === attempt ? "failed" as const : status);
    publish({ ...current, attempts });
    callbacks.current.onError(attempt);
    if (attempts.length <= RASTER_TILE_LOAD_RETRY_LIMIT) recover("error");
    else if (attempts.every((status) => status === "failed")) callbacks.current.onFailed();
  }

  return <>{state.attempts.map((status, attempt) => {
    if (status === "failed" || (state.winner !== null && state.winner !== attempt)) return null;
    // Give original requests precedence over probes, including offscreen tiles.
    // React 18 needs the DOM spelling for this newer image attribute.
    const priority = { fetchpriority: attempt === 0 ? "high" : "low" };
    return <img key={attempt} className="mapTileImage"
      src={attempt === 0 ? props.initialSrc ?? props.src : rasterTileLoadUrl(props.src, attempt)}
      {...priority}
      alt="" draggable={false}
      onLoad={() => complete(attempt, true)} onError={() => complete(attempt, false)} />;
  })}</>;
}
