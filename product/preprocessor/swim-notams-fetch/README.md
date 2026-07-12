# `swim-notams-fetch`

Small standalone Java collector for FAA SWIFT/SCDS NOTAM Distribution queues.

Purpose:
- keep the FAA queue credentials out of the Rust repo
- use the vendor-supported Solace JMS path to drain messages
- write raw captured messages to SQLite so Rust can normalize them into a live feed

Expected credential file:
- dev-stack generated file: `/root/aerobag-credentials/dev-stack/swim-notams.json`
- prod installed file: `/etc/aerobag/secrets/swim-notams.json`

These single-environment files are generated from an operator-owned source bundle:

- `/root/aerobag-credentials/swim-notams.environments.json`

Do not point dev and prod at the same SWIM queue. SWIM subscriptions are stateful
queues; two Aerobag daemons consuming the same queue would split the stream and
neither daemon could reconstruct complete NOTAM state.

Shape:

```json
{
  "aerobagEnvironment": "dev",
  "providerUrl": "tcps://ems1.swim.faa.gov:55443",
  "queue": "jonh.faaswim.jonh.net.AIM_FNS....OUT",
  "connectionFactory": "jonh.faaswim.jonh.net.CF",
  "username": "jonh.faaswim.jonh.net",
  "password": "fill-me-in-locally",
  "vpn": "AIM_FNS",
  "maxMessages": 0,
  "receiveTimeoutMillis": 2000
}
```

Build:

```bash
cd product/preprocessor/swim-notams-fetch
gradle installDist
```

Alternative if Maven is installed:

```bash
cd product/preprocessor/swim-notams-fetch
mvn package
```

Run:

```bash
product/preprocessor/swim-notams-fetch/build/install/swim-notams-fetch/bin/swim-notams-fetch \
  --config /root/aerobag-credentials/dev-stack/swim-notams.json \
  --sqlite /path/to/swim-notams/state/current.sqlite
```

Alternative if using the Maven assembly jar:

```bash
java -jar target/swim-notams-fetch-0.1.0-jar-with-dependencies.jar \
  --config /root/aerobag-credentials/dev-stack/swim-notams.json \
  --sqlite /path/to/swim-notams/state/current.sqlite
```

Output:
- committed rows in SQLite table `raw_notam_messages`

Durability behavior:
- the collector stays connected and waits on the SWIM queue
- for each message, it inserts one `raw_notam_messages` row in a SQLite transaction
- it acknowledges the JMS message only after the SQLite commit succeeds
- `maxMessages = 0` means run forever; positive values are for manual bounded captures

Current scope:
- capture raw queue messages and JMS metadata
- durably commit each captured message before acknowledging it
- leave all NOTAM normalization and live-feed publication to Rust

Daemon integration:

```bash
aerobag-live-feedsd \
  --live-root <live_root> \
  --scratch-root <scratch_root> \
  --listen <addr> \
  --swim-notams-config /root/aerobag-credentials/dev-stack/swim-notams.json \
  --swim-notams-environment dev \
  --swim-notams-collector product/preprocessor/swim-notams-fetch/build/install/swim-notams-fetch/bin/swim-notams-fetch \
  --swim-notams-state-root <artifact_root>/state/swim-notams
```

When enabled, the live-feeds daemon verifies that the credential file declares
the expected environment, runs this collector in an isolated supervisor loop,
applies committed raw SQLite rows into current NOTAM state, and publishes a
`notams` live-feed product from SQLite. Collector failures show up as `notams`
source health in `/live-feeds/status.html` and do not block the other live-feed
products.

Observed live payload shape:
- messages are `SolTextMessage`
- body payload is FAA AIXM 5.1 XML (`AIXMBasicMessage`)
- useful routing/projection hints already exist in JMS properties, for example:
  - `us_gov_dot_faa_aim_fns_nds_ICAOId`
  - `us_gov_dot_faa_aim_fns_nds_LocationDesignator`
  - `us_gov_dot_faa_aim_fns_nds_NOTAMKeyword`
  - `us_gov_dot_faa_aim_fns_nds_NOTAMStatus`
  - `us_gov_dot_faa_aim_fns_nds_SourceType`
- FAA’s portal currently hands out `tcps://...`; the collector normalizes that to the Solace `smfs://...` form internally
