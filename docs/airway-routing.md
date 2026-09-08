# Airway routing

The first version edits one interval of the current flight plan. Select a
standalone waypoint row, choose **Find airways**, then choose a later standalone
waypoint in the action tray. The interval may contain complete airways, but no
procedures. Airway/procedure child rows cannot be boundaries. Applying a draft
replaces the interval as one undoable definition edit; cancel preserves the plan.
Draft via points are temporary editor state, not persistent routing history.

The search follows published low-altitude airways between ordered named via
points. Direct connectors are permitted only at the two interval endpoints when
they are outside the graph. The initial cost is total distance plus five times
direct distance. A candidate must traverse at least one airway edge. Direct
distance is a comparison; an unreasonable airway detour need not be offered.

## NAV25 publication

NAV25 adds `airway/routing/manifest` (schema 1) and numbered
`airway/routing/chunk/00000` records. Each chunk contains at most 128 nodes with
directed adjacency lists. Node IDs are contiguous within one NAVDB publication;
they must not survive a NAVDB epoch change. Shared navigation identities join
airways; geometric crossings alone do not.

The intermediate database now retains AWY1 records in
`airway_segment_metadata`. Directional conventional and GNSS MEAs, maximum
altitudes, crossing constraints and signal-gap metadata accompany each graph
edge. Discontinued and unusable segments are excluded. A missing AWY1 record or
missing intermediate AWY2 position never establishes a connection. Unknown
altitudes remain null. The ordinary airway branch records remain unchanged.

MEAs describe the airway portion, not endpoint direct connectors. The routing
display reports the highest conventional airway MEA and identifies the segment
that imposes it; crossing constraints retain their separate meaning. Routing
does not select procedures or require an altitude ceiling from the pilot.

The NAV25 artifact publication and both compact NAVDB fixtures must be rebuilt
before the client can demonstrate this feature. Fixture locks must reference
the rebuilt artifact repository commit, not merely relabel existing NAV24 data.
