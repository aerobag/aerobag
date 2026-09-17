// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import React, { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { NotamBadgeUiView, NotamDetailUiView } from "./domain/types";
import { TrayScrim } from "./TrayScrim";

export function NotamBadgedControl(props: {
  badge?: NotamBadgeUiView | null;
  active: boolean;
  overlayBadge?: boolean;
  children: React.ReactNode;
  onOpenChange?: (open: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const controlRef = useRef<HTMLDivElement>(null);
  const badge = props.badge;
  const readerOpen = open && props.active && !!badge;
  const previousOpen = useRef(false);
  const onOpenChange = useRef(props.onOpenChange);
  onOpenChange.current = props.onOpenChange;
  useEffect(() => {
    if (previousOpen.current !== readerOpen) {
      previousOpen.current = readerOpen;
      onOpenChange.current?.(readerOpen);
    }
  }, [readerOpen]);
  useEffect(() => {
    if (!props.active || !badge) setOpen(false);
  }, [props.active, badge]);
  useEffect(() => {
    if (!open) return;
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.stopPropagation(); setOpen(false); }
    };
    window.addEventListener("keydown", close, true);
    return () => window.removeEventListener("keydown", close, true);
  }, [open]);
  return <div className={`notamBadgedControl${props.overlayBadge ? " isOverlayBadge" : ""}`} ref={controlRef}>
    {props.children}
    {badge ? <NotamBadgeButton badge={badge} placement="inline" onOpen={() => setOpen(true)} /> : null}
    {open && props.active && badge ? createPortal(
      <div className="notamReaderOverlay">
        <TrayScrim ariaLabel="Close NOTAMs" onClose={() => setOpen(false)} />
        <NotamModal detail={badge.detail} />
      </div>, controlRef.current?.closest(".appShell") ?? document.body,
    ) : null}
  </div>;
}
function stopPointer(event: React.PointerEvent<HTMLElement>) { event.stopPropagation(); }
function stopWheel(event: React.WheelEvent<HTMLElement>) { event.stopPropagation(); }
function stopClick(event: React.MouseEvent<HTMLElement>) { event.stopPropagation(); }
function stopDoubleClick(event: React.MouseEvent<HTMLElement>) { event.preventDefault(); event.stopPropagation(); }
export function NotamSection(props: {
  notams: NotamDetailUiView["notams"];
  label: string;
  trailingLabel: string;
  emptyText: string;
}) {
  return (
    <section className="weatherDetailSection airportNotamSection">
      <div className="weatherDetailSectionTitle">
        <span>{props.label}</span>
        <span>{props.trailingLabel}</span>
      </div>
      <div className="airportNotamList">
        {props.notams.length > 0 ? props.notams.map((notam) => (
          <article className="airportNotamCell" key={notam.id}>
            <div className="airportNotamLabel">{notam.label}</div>
            <div className="airportNotamText">{notam.text}</div>
          </article>
        )) : (
          <div className="airportNotamEmpty">{props.emptyText}</div>
        )}
      </div>
    </section>
  );
}

export function NotamModal(props: {
  detail: NotamDetailUiView;
}) {
  return (
    <section
      className="mapSelectionDetailModal weatherDetailModal procedureNotamDetailModal"
      data-testid="procedure-notam-modal"
      aria-label={props.detail.title}
      onPointerDown={stopPointer}
      onPointerMove={stopPointer}
      onPointerUp={stopPointer}
      onPointerCancel={stopPointer}
      onWheel={stopWheel}
      onClick={stopClick}
      onDoubleClick={stopDoubleClick}
    >
      <div className="mapSelectionDetailTitle">{props.detail.title}</div>
      <div className="weatherDetailAdvisory">{props.detail.advisory_text}</div>
      <div className="weatherDetailSections">
        <NotamSection
          notams={props.detail.notams}
          label="NOTAM"
          trailingLabel={String(props.detail.notams.length)}
          emptyText={props.detail.empty_text}
        />
      </div>
    </section>
  );
}

export function NotamBadgeButton(props: {
  badge: NotamBadgeUiView;
  placement: "folder" | "dock" | "inline";
  onOpen: () => void;
}) {
  return (
    <button
      type="button"
      className={`plateProcedureNotamBadge plateProcedureNotamBadge-${props.placement}`}
      data-testid={`plate-notam:${props.badge.action_id}`}
      aria-label={props.badge.accessibility_label}
      title={props.badge.accessibility_label}
      data-action-id={props.badge.action_id}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={(event) => {
        event.stopPropagation();
        props.onOpen();
      }}
    >
      <span>{props.badge.label}</span>
      <span>{props.badge.count}</span>
    </button>
  );
}
