// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

export const SERVICE_BULLETIN_PATH = "/service/bulletins-v1.json";
export const SERVICE_BULLETIN_CONTROL_PATH = "/__service-notifications";

export const SERVICE_NOTICE_FIXTURES = Object.freeze([
  { id: "journey-info", title: "Journey information notice", body: "Information bulletin received through the live feed.", severity: "info" },
  { id: "journey-caution", title: "Journey caution notice", body: "Caution bulletin received through the live feed.", severity: "caution" },
  { id: "journey-warning", title: "Journey warning notice", body: "Warning bulletin received through the live feed.", severity: "warning" },
  { id: "journey-history", title: "Journey archived notice", body: "Resolved warning retained for explicit history review.", severity: "warning", resolved: true },
].map(Object.freeze));

// This controls real HTTP documents and SSE, never an application session.
export function createServiceBulletinFixture() {
  const subscribers = new Set();
  let revision = 1, publication = "empty", liveAnnouncements = 0;
  const publisherFor = (host) => `http://${host}${SERVICE_BULLETIN_PATH}`;
  const hint = ({ response, host }) => response.write(
    `id: service-bulletins:${revision}\nevent: service-bulletins\ndata: ${JSON.stringify({
      publisher: publisherFor(host), revision,
    })}\n\n`,
  );
  return {
    state() {
      return { revision, publication, subscribers: subscribers.size, live_announcements: liveAnnouncements };
    },
    document(host) {
      return {
        schema_version: 1, publisher: publisherFor(host), revision,
        published_at_utc: "2026-01-01T00:00:00Z", releases: [],
        notices: publication === "empty" ? [] : SERVICE_NOTICE_FIXTURES.map((notice) => ({
          attention_revision: 1, published_at_utc: "2026-01-01T00:00:00Z",
          effective_at_utc: null, expires_at_utc: null, resolved: false,
          audience: { releases: [], platforms: [] }, link: null, ...notice,
          ...(publication === "archived" ? { resolved: true } : {}),
        })),
      };
    },
    subscribe(response, host) {
      const subscriber = { response, host };
      subscribers.add(subscriber);
      response.once("close", () => subscribers.delete(subscriber));
      // Reconnection advertises the latest document, not a stale startup copy.
      hint(subscriber);
    },
    publish(update) {
      if (!update || Object.keys(update).length !== 1 ||
          !["empty", "published", "archived"].includes(update.publication)) {
        throw new Error("service publication must be empty, published, or archived");
      }
      publication = update.publication;
      revision += 1;
      for (const subscriber of subscribers) {
        hint(subscriber);
        liveAnnouncements += 1;
      }
      return this.state();
    },
    reset() {
      // Old app streams must not observe the next journey's publication.
      for (const { response } of subscribers) response.destroy();
      subscribers.clear();
      revision = 1;
      publication = "empty";
      liveAnnouncements = 0;
    },
  };
}
