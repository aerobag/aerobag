#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import argparse
import datetime as dt
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
from http.cookiejar import CookieJar
from pathlib import Path


USER_AGENT = "Mozilla/5.0"
GLOBE_URL = "https://globe.adsb.fi/"
OPEN_DATA_REG_URL = "https://opendata.adsb.fi/api/v2/registration/{registration}"
CURRENT_TRACE_URL = "https://globe.adsb.fi/data/traces/{suffix}/trace_full_{hex}.json"
HISTORIC_TRACE_URL = (
    "https://globe.adsb.fi/globe_history/{year:04d}/{month:02d}/{day:02d}/"
    "traces/{suffix}/trace_full_{hex}.json"
)


CHARSET = "ABCDEFGHJKLMNPQRSTUVWXYZ"
DIGITSET = "0123456789"
ALLCHARS = CHARSET + DIGITSET
SUFFIX_SIZE = 1 + len(CHARSET) * (1 + len(CHARSET))
BUCKET4_SIZE = 1 + len(CHARSET) + len(DIGITSET)
BUCKET3_SIZE = len(DIGITSET) * BUCKET4_SIZE + SUFFIX_SIZE
BUCKET2_SIZE = len(DIGITSET) * BUCKET3_SIZE + SUFFIX_SIZE
BUCKET1_SIZE = len(DIGITSET) * BUCKET2_SIZE + SUFFIX_SIZE


def suffix_offset(s: str) -> int:
    if len(s) == 0:
        return 0
    count = (len(CHARSET) + 1) * CHARSET.index(s[0]) + 1
    if len(s) == 2:
        count += CHARSET.index(s[1]) + 1
    return count


def create_icao(prefix: str, value: int) -> str:
    suffix = hex(value)[2:]
    return prefix + ("0" * (6 - len(prefix) - len(suffix))) + suffix


def n_to_icao(registration: str) -> str | None:
    registration = registration.strip().upper()
    if not registration.startswith("N") or len(registration) < 2 or len(registration) > 6:
        return None
    body = registration[1:]
    if not body[0].isdigit() or body[0] == "0":
        return None

    count = 1
    for index, char in enumerate(body):
        if index == 4:
            if char not in ALLCHARS:
                return None
            count += ALLCHARS.index(char) + 1
            continue
        if char in CHARSET:
            if index < len(body) and any(c not in CHARSET for c in body[index:]):
                return None
            count += suffix_offset(body[index:])
            break
        if char not in DIGITSET:
            return None
        if index == 0:
            count += (int(char) - 1) * BUCKET1_SIZE
        elif index == 1:
            count += int(char) * BUCKET2_SIZE + SUFFIX_SIZE
        elif index == 2:
            count += int(char) * BUCKET3_SIZE + SUFFIX_SIZE
        elif index == 3:
            count += int(char) * BUCKET4_SIZE + SUFFIX_SIZE
    return create_icao("a", count)


def build_opener() -> urllib.request.OpenerDirector:
    jar = CookieJar()
    opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
    opener.addheaders = [("User-Agent", USER_AGENT)]
    return opener


def fetch_bytes(
    opener: urllib.request.OpenerDirector,
    url: str,
    *,
    referer: str | None = None,
) -> bytes:
    headers = {}
    if referer:
        headers["Referer"] = referer
    request = urllib.request.Request(url, headers=headers)
    with opener.open(request, timeout=30) as response:
        return response.read()


def warm_globe_session(opener: urllib.request.OpenerDirector, hex_code: str) -> str:
    page_url = f"{GLOBE_URL}?icao={hex_code.upper()}"
    fetch_bytes(opener, page_url)
    return page_url


def fetch_live_registration(registration: str) -> dict:
    with urllib.request.urlopen(
        urllib.request.Request(
            OPEN_DATA_REG_URL.format(registration=urllib.parse.quote(registration)),
            headers={"User-Agent": USER_AGENT},
        ),
        timeout=30,
    ) as response:
        return json.loads(response.read().decode("utf-8"))


def fetch_trace_for_date(
    opener: urllib.request.OpenerDirector,
    hex_code: str,
    date: dt.date,
    referer: str,
) -> bytes:
    return fetch_bytes(
        opener,
        HISTORIC_TRACE_URL.format(
            year=date.year,
            month=date.month,
            day=date.day,
            suffix=hex_code[-2:],
            hex=hex_code,
        ),
        referer=referer,
    )


def fetch_current_trace(
    opener: urllib.request.OpenerDirector,
    hex_code: str,
    referer: str,
) -> bytes:
    return fetch_bytes(
        opener,
        CURRENT_TRACE_URL.format(suffix=hex_code[-2:], hex=hex_code),
        referer=referer,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Fetch adsb.fi globe trace JSON for a U.S. registration.",
    )
    parser.add_argument("registration", help="U.S. tail number, e.g. N550AR")
    parser.add_argument(
        "--date",
        action="append",
        default=[],
        help="UTC date to try in YYYY-MM-DD form. May be repeated.",
    )
    parser.add_argument(
        "--start-date",
        help="Start UTC date for an inclusive daily range in YYYY-MM-DD form.",
    )
    parser.add_argument(
        "--end-date",
        help="End UTC date for an inclusive daily range in YYYY-MM-DD form.",
    )
    parser.add_argument(
        "--include-current",
        action="store_true",
        help="Also fetch the current day trace from data/traces/.",
    )
    parser.add_argument(
        "--output-dir",
        default="/tmp/adsb-traces",
        help="Directory for saved trace JSON files.",
    )
    return parser.parse_args()


def iter_inclusive_dates(start: dt.date, end: dt.date) -> list[dt.date]:
    step = dt.timedelta(days=1)
    dates: list[dt.date] = []
    current = start
    if start <= end:
        while current <= end:
            dates.append(current)
            current += step
    else:
        while current >= end:
            dates.append(current)
            current -= step
    return dates


def main() -> int:
    args = parse_args()
    registration = args.registration.strip().upper()
    hex_code = n_to_icao(registration)
    if not hex_code:
        print(f"invalid U.S. registration: {registration}", file=sys.stderr)
        return 2

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    print(f"registration={registration}")
    print(f"hex={hex_code.upper()}")

    opener = build_opener()
    referer = warm_globe_session(opener, hex_code)
    print(f"warmed_globe_session={referer}")

    try:
        live = fetch_live_registration(registration)
        print(f"live_total={live.get('total')}")
    except Exception as exc:  # pragma: no cover - network diagnostics
        print(f"live_lookup_error={exc}")

    dates = [dt.date.fromisoformat(raw) for raw in args.date]
    if args.start_date and args.end_date:
        dates.extend(
            iter_inclusive_dates(
                dt.date.fromisoformat(args.start_date),
                dt.date.fromisoformat(args.end_date),
            )
        )
    elif args.start_date or args.end_date:
        print("both --start-date and --end-date are required together", file=sys.stderr)
        return 2
    seen_dates: set[dt.date] = set()
    deduped_dates: list[dt.date] = []
    for value in dates:
        if value in seen_dates:
            continue
        seen_dates.add(value)
        deduped_dates.append(value)
    dates = deduped_dates
    if args.include_current:
        try:
            current_bytes = fetch_current_trace(opener, hex_code, referer)
            current_path = output_dir / f"{registration.lower()}-current.json"
            current_path.write_bytes(current_bytes)
            try:
                payload = json.loads(current_bytes.decode("utf-8"))
                current_points = len(payload.get("trace", [])) if isinstance(payload.get("trace"), list) else "unknown"
            except Exception:
                current_points = "unknown"
            print(f"current_points={current_points} path={current_path}")
        except urllib.error.HTTPError as exc:
            print(f"current_points=0 http_error={exc.code}")
        except Exception as exc:  # pragma: no cover - network diagnostics
            print(f"current_points=0 error={exc}")

    for date in dates:
        print(f"trying_date={date.isoformat()}")
        try:
            trace_bytes = fetch_trace_for_date(opener, hex_code, date, referer)
            output_path = output_dir / f"{registration.lower()}-{date.isoformat()}.json"
            output_path.write_bytes(trace_bytes)
            point_count: int | str = "unknown"
            try:
                payload = json.loads(trace_bytes.decode("utf-8"))
                trace = payload.get("trace")
                if isinstance(trace, list):
                    point_count = len(trace)
            except Exception:
                point_count = "unknown"
            print(f"date={date.isoformat()} points={point_count} path={output_path}")
        except urllib.error.HTTPError as exc:
            print(f"date={date.isoformat()} points=0 http_error={exc.code}")
        except Exception as exc:  # pragma: no cover - network diagnostics
            print(f"date={date.isoformat()} points=0 error={exc}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
