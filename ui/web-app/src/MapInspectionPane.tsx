// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, useRef, type PropsWithChildren } from "react";
import type { MapInspectionCommand } from "./generated/sessionPageWire";

/** A map hit target, not a second gesture implementation. Capture stays on the map. */
export function MapInspectionBackdrop(props: { onDismiss: () => void }) {
  return <button type="button" className="trayScrim" aria-label="Close map selection"
    style={{ touchAction: "none" }}
    onClick={(event) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.detail === 0) props.onDismiss();
    }} />;
}

/** Report activity to core; child controls/scrolling never become map gestures. */
export function MapInspectionPane(props: PropsWithChildren<{
  onCommand: (command: MapInspectionCommand) => void;
}>) {
  const pointers = useRef(new Set<number>()).current;
  const command = useRef(props.onCommand);
  command.current = props.onCommand;
  useEffect(() => {
    const release = (event: PointerEvent) => {
      if (pointers.delete(event.pointerId) && pointers.size === 0) command.current("touch_ended");
    };
    window.addEventListener("pointerup", release, true);
    window.addEventListener("pointercancel", release, true);
    return () => {
      window.removeEventListener("pointerup", release, true);
      window.removeEventListener("pointercancel", release, true);
      if (pointers.size) command.current("touch_ended");
      pointers.clear();
    };
  }, [pointers]);
  return <div style={{ display: "contents" }}
    onPointerDownCapture={(event) => {
      if (!pointers.size) props.onCommand("touch_started");
      pointers.add(event.pointerId);
    }}
    onPointerDown={(event) => event.stopPropagation()}
    onPointerMove={(event) => event.stopPropagation()}
    onPointerUp={(event) => event.stopPropagation()}
    onPointerCancel={(event) => event.stopPropagation()}
    onWheelCapture={() => props.onCommand("activity")}
    onWheel={(event) => event.stopPropagation()}
    onKeyDownCapture={() => props.onCommand("activity")}
    onClickCapture={() => props.onCommand("activity")}
    onDoubleClick={(event) => event.stopPropagation()}
  >{props.children}</div>;
}
