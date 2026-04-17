import type {
  AirwayEntryCandidate,
  AirwayExitCandidate,
  AirwayPresentationPlan,
} from "./types";

export function airwayEntryCandidateFromPresentation(
  presentation: AirwayPresentationPlan,
  pointIndex: number,
): AirwayEntryCandidate {
  const point = presentation.points[pointIndex];
  if (!point) {
    throw new Error(`airway entry index out of range: ${pointIndex}`);
  }
  return {
    airway_name: presentation.airway_name,
    branch_key: presentation.branch_key,
    branch_point_index: point.branch_point_index,
    sequence: point.sequence,
    nav_ref: point.nav_ref,
    distance_from_anchor_nm: 0,
    previous_nav_ref: pointIndex > 0 ? presentation.points[pointIndex - 1]?.nav_ref ?? null : null,
    next_nav_ref: pointIndex + 1 < presentation.points.length ? presentation.points[pointIndex + 1]?.nav_ref ?? null : null,
  };
}

export function airwayExitCandidatesFromPresentation(
  presentation: AirwayPresentationPlan,
  entryIndex: number,
): AirwayExitCandidate[] {
  return presentation.points.map((point, pointIndex) => ({
    airway_name: presentation.airway_name,
    branch_key: presentation.branch_key,
    branch_point_index: point.branch_point_index,
    sequence: point.sequence,
    nav_ref: point.nav_ref,
    leg_offset_from_entry: pointIndex - entryIndex,
    is_entry: pointIndex === entryIndex,
    distance_from_target_nm: null,
  }));
}
