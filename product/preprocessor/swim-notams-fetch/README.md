# `swim-notams-fetch`

Small standalone Java collector for FAA SWIFT/SCDS NOTAM Distribution queues.

Purpose:
- keep the FAA queue credentials out of the Rust repo
- use the vendor-supported Solace JMS path to drain messages
- write raw captured messages as JSONL so Rust can normalize them later into a live feed

Expected credential file:
- `/root/aerobag-credentials/swim-notams.json`

Shape:

```json
{
  "providerUrl": "tcps://ems1.swim.faa.gov:55443",
  "queue": "jonh.faaswim.jonh.net.AIM_FNS....OUT",
  "connectionFactory": "jonh.faaswim.jonh.net.CF",
  "username": "jonh.faaswim.jonh.net",
  "password": "fill-me-in-locally",
  "vpn": "AIM_FNS",
  "maxMessages": 100,
  "idleExitAfterMillis": 15000,
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
  --config /root/aerobag-credentials/swim-notams.json \
  --output-dir /tmp/swim-notams-raw
```

Alternative if using the Maven assembly jar:

```bash
java -jar target/swim-notams-fetch-0.1.0-jar-with-dependencies.jar \
  --config /root/aerobag-credentials/swim-notams.json \
  --output-dir /tmp/swim-notams-raw
```

Outputs:
- `messages.jsonl`
- `summary.json`

Current scope:
- capture raw queue messages and JMS metadata
- do not attempt to normalize NOTAM schema yet
- this is the staging step before adding a Rust-side `notams` live feed

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
