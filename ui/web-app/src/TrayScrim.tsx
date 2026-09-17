// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import type React from "react";
function stopPointer(event: React.PointerEvent<HTMLElement>) { event.stopPropagation(); }
function stopDoubleClick(event: React.MouseEvent<HTMLElement>) { event.preventDefault(); event.stopPropagation(); }
export function TrayScrim(props: { ariaLabel: string; onClose: () => void }) {
  function handlePointerDown(event: React.PointerEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
    props.onClose();
  }

  function handleClick(event: React.MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
    if (event.detail === 0) {
      props.onClose();
    }
  }

  return (
    <button
      type="button"
      className="trayScrim"
      aria-label={props.ariaLabel}
      onPointerDown={handlePointerDown}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={handleClick}
    />
  );
}
