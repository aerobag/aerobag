# Service Notifications Journey

`shared.service-notifications` is a shared p0 web/Android journey. It replaces
the old separate Service Notifications Home destination/page coverage.

## Contract

The journey starts with an empty bulletin, waits for the app's completed initial
HTTP fetch and an open live-feed SSE stream, then publishes three unread notices
(info, caution, warning) and one resolved warning. Publication changes real
fixture HTTP bytes and broadcasts `service-bulletins` on the existing SSE
connection. There is no app-state injection, reload, manual refresh, or clock
jump to obtain the new notices.

Physical semantic actions open `/!\`, choose **Read notifications**, and require
both `parity:page:data_status` and dismissal of the status popup. Notice titles
are discovered by rendered text; receipt hashes are taken from their control
tags, never duplicated from core's receipt algorithm. The journey opens and
reveals each body, checks its visible text, and verifies that opening one notice
does not mark the others read. After all three are read, it scans the rendered
status panel, including Android scrolling, to prove the service aggregate is
gone rather than merely offscreen. If there are no unrelated faults, the entire
launcher may disappear instead: stable observations require the map to be visible,
both visible and projected launcher nodes to be absent, and no popup to remain.
A projected but offscreen launcher still requires opening and scanning the panel.
Reentering Status must fold the whole section;
explicit expansion must retain readable read and archived history.

Required tags on both platforms are `parity:service:section`,
`parity:service:toggle`, `parity:service:mark-all-read`,
`parity:service:notice:<receipt hash>`, and `parity:service:body:<receipt hash>`.
Web's status tags are `data-status-launcher`, `data-status-panel`,
`data-status-box-<box id>`, and `data-status-action-<box id>-<action id>`;
Android uses the same tags prefixed with `parity:` and indexed actions.
The service box/action IDs are `service:unread` and `service:inbox`.
Expansion is observed through web `aria-expanded` or Android's indexed section
state `expanded:true/false`, not the header's visible title/summary.
Renderer component tests own tone colors and absence of history highlights;
this journey does not claim pixel-level style verification.

## Fixture Control

`POST /__service-notifications` accepts exactly
`{"publication":"published"}`, `{"publication":"archived"}`, or
`{"publication":"empty"}`. Each update increments the bulletin revision and
broadcasts a hint using each SSE client's Host, including adb-reversed ports.
Reconnecting streams receive the latest revision. `GET /__health` reports
`service_notifications` revision, publication, subscriber count and live
announcement deliveries; `GET /__requests` records the revision actually served.

Ordinary journeys receive an empty revision-1 bulletin. `POST /__control` with
`{"reset":true}` restores that default, closes old SSE connections and rejects
publication requests spanning the reset boundary. The journey resets before
starting the app and in `finally`, including failures.

## Running

Cheap checks use ephemeral localhost servers, without app builds or emulators:

```sh
node --test tools/e2e/service-notifications-fixture.test.mjs tools/e2e/service-notifications-journey.test.mjs tools/e2e/fixture-isolation.test.mjs
```

After rebuilding the matching apps, run the focused journey in an exclusively
owned fixture lane. Set `PACKAGE_SOURCE_PORT`, `AEROBAG_RELEASE_JOURNEY_FIXTURE`,
`AEROBAG_RELEASE_JOURNEY_WEB_DIST`, and `ANDROID_SERIAL` explicitly rather than
reusing another session's global fixture, dist, or emulator:

```sh
tools/e2e/release_journey_lab.sh web-dist-test shared.service-notifications
tools/e2e/release_journey_lab.sh android-test shared.service-notifications
```

Harness-model passes establish orchestration and defect rejection only. A real
run against the updated app is still required on each claimed platform.
