# Receiver Vendor Protocol Blobs

## Protocol Inputs

Use **vendor protocol blobs**, with two role-specific names:

| Name | Observed size | Observed role |
| --- | ---: | --- |
| Challenge seed | 48 bytes | Input to the protocol's challenge-response calculations |
| Setup payload | 694 bytes | Sent verbatim in application message ID 1 after the challenge-response exchange |

These are shared vendor setup data. The protocol was reconstructed by observing
and reproducing Bluetooth communication.
Setup uses a challenge-response exchange with response validation and ordered
message delivery.

On October 11, 2026, another user extracted both blobs from their independently
installed Garmin Pilot application. The user supplied this comparison:

| Blob | SHA-256 |
| --- | --- |
| Challenge seed | `8f01ff2f447c06ca5c399e074a59e0e12299aab3038917848658b07fef088771` |
| Setup payload | `b4042308b8f6230eadfca5da903055a5fc84928755d92d1fe2f690967d78e932` |

Both hashes and lengths exactly match the Aerobag development copies. The
comparison establishes equality across these two installations, not invariance
across every past/future Pilot version or compatibility with every receiver.
The internal meaning and necessity of the setup payload remain undetermined.

`VendorProtocolBlobs` owns the protocol inputs in core. The UI calls them
"challenge seed" and "setup payload". Device-local filenames are
`vendor-challenge-seed.bin` and `vendor-setup-payload.bin`; development copies
live under `/root/aerobag-vendor/gtx345/`. Existing tablet installations receive a
one-time filename migration that preserves their imported blob contents.

App-private storage remains appropriate for ordinary app-owned files. Flight
captures remain private because they contain locations, aircraft/device
identifiers, and flight history. This build imports the blobs locally rather than
bundling them in the APK or cloud account.

## Current Hardware Result

The October 10 cockpit test used both blobs. Its exported capture contains a
24.3-second connected interval with sensor messages and empty responses from all
11 weather channels, and replays without corruption or product decoding errors.
It did **not** test omission of the setup payload. No nonempty weather or valid
receiver GPS fix was captured; the test was deliberately short and stationary.

## Setup-Payload Omission Experiment

Status: implemented for the next hardware test. Normal remains the default;
omission has only been exercised with synthetic peers, not a GTX 345.

Question: can this receiver/firmware deliver the same requested data without
sending application message ID 1? In particular, sensor reception alone does not
answer whether weather delivery requires that message.

### Core-Owned Modes

- **Normal: send setup payload.** The current two-blob sequence, default on every
  app restart. Both imported blobs remain installed in either mode.
- **Experiment: omit setup payload.** Identical challenge seed, challenge-response
  validation, supported-ID discovery, subscriptions, and data handling. Omit the
  entire message ID 1, not an empty substitute or a delayed transmission.

Use the existing Settings receiver panel. Only allow mode selection while
disconnected. Latch the mode for a complete connection and its automatic
reconnects. Display the selected mode during setup, reception, and failures.
Provide a single explicit action to disconnect and return to Normal. Never
silently fall back to Normal: that would contaminate the experiment's evidence.
Do not clear either blob, weather caches, or earlier captures when switching.
Platforms only render the core's labels/actions and execute OS operations.

ABRX v2 records selected mode, app build and blob lengths/hashes durably before
each socket open. The receiver's product-identity response remains in raw RX;
its firmware fields are not yet interpreted. Export includes this evidence with
raw RX/TX. The reader also supports ABRX v1, without inventing mode metadata.
`receiver_capture_replay` reports each connection's reception counters and
completed setup-payload transmissions. It rejects an experimental capture that
actually transmitted message ID 1, or a payload with the wrong fingerprint.
Uncompleted socket writes remain explicitly uncertain, not evidence of absence.

Show per-connection progress for sensor reports and for each weather family:
empty/nonempty response counts, received bytes, and last arrival. Report newly
received payloads separately from restored/displayed cache contents. This makes
"it connects, but only receives empty weather" observable without guessing from
the map. Recording must continue independently of UI delivery and decoding.

### Hardware Procedure

1. At home, validate the mode controls and preserved Normal path with synthetic
   peers and on the tablet. Verify available capture space and independent radar
   archival before the flight. Do not make a flight depend on experiment success.
2. With the aircraft parked, begin with a normally cold-started receiver and
   **Experiment first**, before Pilot or any Normal Aerobag session has supplied
   the payload. Record whether handshake, identity, and sensors arrive. A prior
   Normal session may leave receiver-side state; warm success alone is not proof
   that the payload is unnecessary. Do not send reset/configuration commands.
3. If setup fails, stop the attempt and explicitly return to Normal. Preserve
   the failure evidence and use Normal for the rest of the data-collection flight.
   Existing bounded protocol setup deadlines should surface failure; no repeated
   manual tapping or experimental retries are needed.
4. If Experiment receives sensors, retain it until ground-station coverage is
   available. A passenger/operator, not a busy pilot, observes whether genuinely
   nonempty weather arrives over this connection. Lack of coverage is inconclusive.
5. If convenient and safe, compare Experiment -> Normal -> Experiment over fresh
   Bluetooth connections during the same coverage interval. Each run must request
   the same products. If Experiment has no weather but Normal does, that is useful
   evidence, not conclusive proof: coverage and product availability also change.
   Finish in Normal so the trip still gathers the known-working configuration's
   traffic/weather stream. Do not power-cycle avionics aloft for this test.
6. Export and compare the session-labelled captures. Any Experiment success only
   after Normal ran is explicitly classified as a warm-state result. If needed,
   repeat Experiment-first at the next normal cold start; power cycling might not
   clear persistent receiver state, so retain that limitation in conclusions.

### Interpretation

| Observation | Conclusion |
| --- | --- |
| No setup completion without payload; Normal works | Keep payload for this receiver/firmware; inspect ordering and responses |
| Sensors but no nonempty weather in either mode | Transport proven; weather necessity inconclusive |
| Nonempty weather in Normal but not Experiment | Suspected dependency; check request parity, timing, coverage, and receiver state |
| Nonempty weather in Experiment only after Normal | Warm-session compatibility; possible retained receiver state |
| Sensors and nonempty weather in Experiment-first | Payload not required for observed products on this tested receiver/firmware; not a universal compatibility claim |

### Software Coverage

- Compare synthetic-peer transcripts: the sole semantic difference is omission
  of message ID 1. The challenge response, supported-ID filtering, product
  subscriptions, and weather-notification fetch requests stay unchanged.
- A peer that requires the payload must fail visibly in Experiment; a peer that
  does not require it must deliver sensor and nonempty text/radar products through
  the real runtime/session path. These prove our implementation, not hardware.
- Switching modes never deletes setup data or caches, stale socket callbacks
  cannot cross sessions, and automatic retries cannot change modes.
- New process startup restores Normal; activity rotation does not mutate the
  current connection's mode. Controls cannot alter a live handshake halfway.
- Export/replay retains mode metadata, proves the actual omitted/sent message,
  and separates newly received weather from cached weather.
- Physical-tap Android tests exercise mode selection and return to Normal using
  the existing core-owned receiver panel, rather than a new platform policy path.
