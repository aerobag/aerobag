// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { Component, createRef, type ReactNode } from "react";

type Props = { slot: "active" | "standby"; band: ReactNode; children: ReactNode };
type ClosingEditor = { element: HTMLElement; height: number; scroll: Array<[HTMLElement, number, number]> };

// Retain only pixels/markup for the closing editor, never a second interactive
// React tree with stale handlers, refs, or resource-loading effects.
export class FlightPlanWorkspaceLayout extends Component<Props> {
  private frame = createRef<HTMLDivElement>();
  private editor = createRef<HTMLDivElement>();
  private band = createRef<HTMLDivElement>();
  private divider = createRef<HTMLDivElement>();
  private readiness = createRef<HTMLSpanElement>();
  private animations: Animation[] = [];
  private ghost: HTMLElement | null = null;
  private resize: ResizeObserver | null = null;
  private height = 0;

  private finish = () => {
    this.animations.forEach(animation => animation.cancel());
    this.animations = [];
    this.ghost?.remove();
    this.ghost = null;
    if (this.frame.current) this.frame.current.dataset.transition = "idle";
    this.readiness.current?.setAttribute("data-testid", "parity:plan-workspace-transition:idle");
    if (this.editor.current) this.editor.current.inert = false;
    if (this.band.current) this.band.current.inert = false;
  };

  private measure = () => {
    const frame = this.frame.current, band = this.band.current;
    if (!frame || !band) return;
    const height = frame.clientHeight;
    this.height = height;
    const collapsed = Math.min(band.scrollHeight, height / 2);
    frame.style.setProperty("--plan-band-height", `${collapsed}px`);
    frame.style.setProperty("--plan-editor-height", `${Math.max(0, height - collapsed - 8)}px`);
  };

  componentDidMount() {
    this.measure();
    this.resize = new ResizeObserver(() => {
      if (this.frame.current?.clientHeight !== this.height) { this.finish(); this.measure(); }
    });
    this.resize.observe(this.frame.current!);
  }

  componentWillUnmount() { this.resize?.disconnect(); this.finish(); }

  getSnapshotBeforeUpdate(previous: Props): ClosingEditor | null {
    if (previous.slot === this.props.slot) return null;
    this.finish();
    const editor = this.editor.current;
    if (!editor || !editor.clientHeight || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return null;
    const element = editor.cloneNode(true) as HTMLElement;
    const originals = [editor, ...editor.querySelectorAll<HTMLElement>("*")];
    const copies = [element, ...element.querySelectorAll<HTMLElement>("*")];
    const scroll: ClosingEditor["scroll"] = [];
    copies.forEach((copy, index) => {
      for (const attribute of [...copy.attributes]) {
        if (attribute.name === "id" || attribute.name.startsWith("data-") || attribute.name === "name") copy.removeAttribute(attribute.name);
      }
      scroll.push([copy, originals[index].scrollLeft, originals[index].scrollTop]);
      if (copy instanceof HTMLTextAreaElement || copy instanceof HTMLInputElement) {
        copy.value = (originals[index] as HTMLTextAreaElement | HTMLInputElement).value;
      }
    });
    element.inert = true;
    element.setAttribute("aria-hidden", "true");
    element.classList.add("planWorkspaceClosingEditor");
    (element.firstElementChild as HTMLElement).style.height = `${editor.clientHeight}px`;
    return { element, height: editor.clientHeight, scroll };
  }

  componentDidUpdate(previous: Props, _state: unknown, closing: ClosingEditor | null) {
    if (previous.slot === this.props.slot || !closing) return;
    this.measure();
    const frame = this.frame.current!, editor = this.editor.current!, band = this.band.current!, divider = this.divider.current!;
    const expanded = editor.clientHeight, collapsed = band.clientHeight;
    if (expanded <= collapsed || typeof editor.animate !== "function") return;
    const active = this.props.slot === "active";
    const top = collapsed + 8, bottom = expanded + 8;
    this.ghost = closing.element;
    frame.append(closing.element);
    closing.scroll.forEach(([element, left, top]) => { element.scrollLeft = left; element.scrollTop = top; });
    frame.dataset.transition = "running";
    this.readiness.current?.setAttribute("data-testid", "parity:plan-workspace-transition:running");
    editor.inert = true;
    band.inert = true;
    const animate = (element: HTMLElement, from: Keyframe, to: Keyframe, middle?: Keyframe) => {
      const animation = element.animate(middle ? [from, middle, to] : [from, to], { duration: 320, easing: "cubic-bezier(0.4, 0, 0.2, 1)", fill: "both" });
      animation.startTime = document.timeline.currentTime;
      this.animations.push(animation);
    };
    animate(editor, { top: `${active ? 0 : closing.height + 8}px`, height: `${collapsed}px`, opacity: 0 },
      { top: `${active ? 0 : top}px`, height: `${expanded}px`, opacity: 1 });
    animate(closing.element, { top: `${active ? top : 0}px`, height: `${closing.height}px`, opacity: 1 },
      { top: `${active ? bottom : 0}px`, height: `${collapsed}px`, opacity: 0 }, { opacity: 0, offset: 0.5 });
    animate(band, { top: `${active ? top : 0}px`, opacity: 0 }, { top: `${active ? bottom : 0}px`, opacity: 1 }, { opacity: 0, offset: 0.5 });
    animate(divider, { top: `${active ? collapsed : closing.height}px` }, { top: `${active ? expanded : collapsed}px` });
    const animations = this.animations;
    void Promise.all(animations.map(animation => animation.finished)).then(() => {
      if (this.animations === animations) this.finish();
    }, () => {});
  }

  render() {
    return <div ref={this.frame} className={`planWorkspaceLayout is${this.props.slot}`} data-testid="plan-workspace-layout" data-transition="idle">
      <div ref={this.editor} className="planWorkspaceEditorClip" data-testid="plan-workspace-editor-clip">
        <div className="planWorkspaceEditor">{this.props.children}</div>
      </div>
      <div ref={this.band} className="planWorkspaceBandPosition">{this.props.band}</div>
      <div ref={this.divider} className="planWorkspaceDivider" data-testid="plan-workspace-divider" />
      <span ref={this.readiness} className="planWorkspaceReadiness" data-testid="parity:plan-workspace-transition:idle" aria-hidden="true" />
    </div>;
  }
}
