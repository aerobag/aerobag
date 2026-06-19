---
id: TASK-130
title: Idle mode
assignee: []
created_date: '2026-06-02 18:15'
labels:
  - cat:performance
dependencies: []
state: done
ordinal: 130000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
If a web page is unfocused or hasn't experienced a click or drag in an hour, make it idle. Idle pages should disconnect from the live feed so we're not burning resources.

(Maybe someday we'll support a "lightweight idle" for displaying metars..)

Upon wakeup (focus, mouse click?), we reestablish live-feeds.
Maybe we should have a UI element to indicate idleness. I guess that'll happen automatically in the /!\ warning because live feed disconnects?
<!-- SECTION:DESCRIPTION:END -->

## Acceptance

<!-- SECTION:ACCEPTANCE:BEGIN -->
- Web pages enter idle immediately when hidden.
- Visible but unfocused web pages keep the normal inactivity timeout.
- Web pages enter idle after the configured timeout without coarse user activity.
- Idle pages stop the live-feed subscription through the existing session adapter path.
- Focus or user activity wakes the page and restarts the live-feed subscription through the existing session adapter path.
<!-- SECTION:ACCEPTANCE:END -->
