import {
  isStaleMapFollowTargetViewport,
  type MapViewportState,
} from "./mapViewport";

export class MapFollowTargetGate {
  private awaitedTargetViewport: MapViewportState | null = null;

  beginSync(requestedViewport: MapViewportState) {
    this.awaitedTargetViewport = requestedViewport;
  }

  acknowledgeSyncSnapshot(snapshot: {
    following: boolean;
    targetViewport: MapViewportState | null;
  }) {
    if (!snapshot.following) {
      this.awaitedTargetViewport = null;
      return;
    }
    if (snapshot.targetViewport) {
      this.awaitedTargetViewport = snapshot.targetViewport;
    }
  }

  clear() {
    this.awaitedTargetViewport = null;
  }

  awaitedViewport() {
    return this.awaitedTargetViewport;
  }

  shouldApplyTarget(targetViewport: MapViewportState) {
    if (isStaleMapFollowTargetViewport(targetViewport, this.awaitedTargetViewport)) {
      return false;
    }
    this.awaitedTargetViewport = null;
    return true;
  }
}
