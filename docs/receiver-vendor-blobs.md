# Receiver Protocol Compatibility Data

## Bundled Constant

This 48-byte protocol compatibility constant is used in the GTX 345
challenge-response handshake to establish communication with the transponder.
Validated through reception of traffic, sensor, and weather data.

Core bundles it in
`ui/core-rust/crates/app-core/src/receiver/compatibility.rs`.

There is one connection path: answer the challenge using the bundled constant,
validate the receiver's response, discover supported products, and subscribe.
Aerobag does not send application message ID 1, including during automatic
reconnection. There is no alternate two-blob mode, imported-file override, or
silent fallback. Existing experimental files do not affect connections.

The user pairs the receiver in Android Bluetooth settings, grants Aerobag nearby
device permission, refreshes the paired-device list, and chooses Connect. There
are no protocol-file imports. Bluetooth connections do not start automatically
on app restart.

## Protocol Mechanics

Each response is `MD5(constant || challenge)`: the client answers the receiver's
challenge and validates the receiver's answer to its own nonce.

## Hardware Coverage

The October 10 parked test used both blobs and received sensor data but only
empty weather responses. The October 19 flight used only the 48-byte constant
throughout its 30m38s connection, receiving traffic, sensors, METARs, TAFs and
radar. Replay of completed outbound writes confirms no message ID 1 was sent.

This proves reception for the tested GTX 345, its firmware and those products,
not compatibility with every receiver or firmware version. The capture retains
the receiver's product-identity response; its firmware fields are not yet
interpreted. Radar geographic accuracy still needs independent comparison.

## Capture Evidence

While recording is healthy, ABRX v2 records app build and protocol fingerprints
durably before each socket open. New captures use the frozen `omit_setup_payload`
metadata value and an empty-payload fingerprint. The reader still understands
historical v2 `normal` captures and v1 captures without setup metadata.

`receiver_capture_replay` checks completed outbound writes against that metadata,
reports reception counts, and distinguishes uncertain incomplete writes from
successful transmissions. Received weather is counted separately from restored
cache contents. Flight captures contain locations and device identifiers; keep
them outside public artifact serving and Git.

Capture open/write/checkpoint failures or the 256 MiB limit stop recording with
an explicit diagnostic, not Bluetooth reception or product delivery. Existing
capture prefixes are retained, and a failed recording never silently resumes
with a missing interval.

## Software Coverage

- Fixed known-answer tests pin the constant's digest and challenge responses.
- Fresh installs connect without imports or experiment controls; leftover,
  invalid experimental files cannot override the bundled constant.
- Synthetic peers exercise handshake, subscriptions, sensors, traffic, nonempty
  METAR/radar delivery, cache restoration, reconnects and session replacement.
- Completed-transmission replay proves absence of message ID 1, while historical
  captures still verify its fingerprint when present.
- Recording-failure tests prove reception continues and the UI reports the
  failure. Android physical-tap tests exercise the shared receiver action panel.

Software tests validate implementation behavior; they do not extend the hardware
coverage described above.
