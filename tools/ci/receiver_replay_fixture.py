# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Synthetic received protocol bytes. No captured flight or manufacturer data."""
import base64
from datetime import datetime, timedelta, timezone
import math
import struct
import zlib


def container(parts):
    return (bytes([1, 1, 1, 0]) + bytes(4)
            + struct.pack("<16I", *[len(part) for part in parts], *([0] * (16 - len(parts))))
            + b"".join(parts))


def receiver_replay_fixture():
    start = datetime(2026, 10, 10, 12, tzinfo=timezone.utc)
    latitude, longitude = 47.493, -122.216
    events = [{"elapsed_ms": 0, "type": "connection", "connected": True}]

    def received(second, message_id, payload):
        events.append({"elapsed_ms": second * 1000, "type": "receiver",
                       "message_id": message_id, "bytes_base64": base64.b64encode(payload).decode()})

    for second in range(46):
        payload = bytearray(268)
        struct.pack_into("<H", payload, 0, 1)
        payload[24:26] = bytes([10, 10])
        struct.pack_into("<HhBBdd", payload, 26, 2026, 12, 0, second,
                         math.radians(latitude), math.radians(longitude))
        struct.pack_into("<5f", payload, 48, 120, 90, 90, 2000, 2400)
        payload[89:97] = b"FIXTURE "
        struct.pack_into("<I", payload, 108, 1 | 2 | 4 | 8 | 32)
        payload[113] = 1
        struct.pack_into("<III", payload, 116, 1, 7, 0x123456)
        payload[131] = 1
        struct.pack_into("<dd", payload, 148, math.radians(latitude + .02), math.radians(longitude + .02))
        struct.pack_into("<ff", payload, 168, 2500, 2900)
        struct.pack_into("<f", payload, 196, 110)
        payload[248:256] = b"TRAFFIC "
        struct.pack_into("<I", payload, 264, 64 | 512 | 2048 | 4096 | 2)
        received(second, 0x20000000, payload)

    observed = start - timedelta(minutes=5)
    text = f"METAR KSEA {observed:%d%H%M}Z 00000KT 10SM CLR 11/08 A3000 RMK RECEIVER FIXTURE".encode()
    index = bytearray(77)
    index[:4] = b"KSEA"
    struct.pack_into("<I", index, 69, len(text))
    received(5, 0x20000010, container([bytes([10, 12, 0, 1, 0]), zlib.compress(index), zlib.compress(text)]))
    for second, age, value in [(5, 7, 24), (12, 2, 7)]:
        observed = start - timedelta(minutes=age)
        semicircle = lambda value: round(value * 2**31 / 180)
        metadata = struct.pack("<ii6Bi4H", semicircle(latitude), semicircle(longitude),
                               10, 12, 0, observed.day, observed.hour, observed.minute,
                               semicircle(latitude), 5, 5, 2000, 2000)
        runs = b"".join(struct.pack("<HB", 5, value) for _ in range(5))
        offsets = struct.pack("<5I", *[row * 3 for row in range(5)])
        received(second, 0x20000090, container([metadata, zlib.compress(runs), zlib.compress(offsets)]))
    events.append({"elapsed_ms": 46000, "type": "connection", "connected": False})
    events.sort(key=lambda event: event["elapsed_ms"])
    return {"format": "aerobag-receiver-replay", "schema_version": 1, "title": "Synthetic receiver",
            "start_epoch_ms": int(start.timestamp() * 1000), "duration_ms": 90000, "events": events}


if __name__ == "__main__":
    import json
    print(json.dumps(receiver_replay_fixture(), separators=(",", ":")))
