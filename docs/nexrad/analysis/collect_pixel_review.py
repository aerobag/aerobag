#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Read-only production evidence collection; all writes stay in --capture-dir."""
import argparse
import bisect
from datetime import datetime, timedelta, timezone
import hashlib
import json
from pathlib import Path
import re
import shlex
import shutil
import subprocess

PREFIX = 'channel.production.live_feed.nexrad.'
URL = 'https://mrms.ncep.noaa.gov/data/RIDGEII/L2/CONUS/CREF_QCD/'


def ssh(host, command, **kwargs):
    return subprocess.run(['ssh', '-o', 'BatchMode=yes', host, command],
                          check=True, stdout=subprocess.PIPE, **kwargs).stdout


def scalar(value):
    return value['value'] if isinstance(value, dict) else value


def timestamp(value):
    return datetime.fromisoformat(value.replace('Z', '+00:00')).timestamp()


def read_records(path):
    with path.open() as stream:
        for line in stream:
            yield json.loads(line)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture-dir', type=Path, required=True)
    parser.add_argument('--start', required=True, help='Inclusive ISO-8601 UTC timestamp')
    parser.add_argument('--end', required=True, help='Exclusive ISO-8601 UTC timestamp')
    parser.add_argument('--host', default='root@aerobag-prod.iac.jonh.net')
    parser.add_argument('--remote-data-root', default='/mnt/aerobag-data')
    parser.add_argument('--remote-checkout', default='/opt/aerobag')
    parser.add_argument('--local-fetch-cache', type=Path, default=Path('/root/aerobag-artifacts/cache/fetch'))
    args = parser.parse_args()
    start = datetime.fromisoformat(args.start.replace('Z', '+00:00'))
    end = datetime.fromisoformat(args.end.replace('Z', '+00:00'))
    if start.utcoffset() != timedelta(0) or end.utcoffset() != timedelta(0) or end <= start:
        parser.error('start/end must be UTC and end must follow start')
    root = args.capture_dir.resolve()
    evidence = root / 'evidence'
    if evidence.exists():
        parser.error('evidence directory already exists; use a fresh capture directory')
    evidence.mkdir(parents=True)
    host = args.host
    cache = args.remote_data_root.rstrip('/') + '/artifacts/cache/fetch'
    day = start.date()
    while day <= (end - timedelta(microseconds=1)).date():
        name = f'pipeline_health-{day.isoformat()}.jsonl'
        subprocess.run(['scp', f'{host}:{args.remote_data_root}/health/{name}', str(evidence / name)], check=True)
        day += timedelta(days=1)
    samples = []
    for path in sorted(evidence.glob('pipeline_health-*.jsonl')):
        for record in read_records(path):
            if not start.timestamp() <= timestamp(record['sampled_at_utc']) < end.timestamp():
                continue
            count = scalar(record['metrics'].get(PREFIX + 'poor_color_match_count', 0))
            if not count:
                continue
            product_path = record['channel_product_states']['production']['product_facts_key'][0]
            release, commit = re.search(r'/published/(\d{4}-\d{2}-\d{2}\.\d+)-([0-9a-f]+)/', product_path).groups()
            samples.append(dict(sampled_at_utc=record['sampled_at_utc'], release=release,
                                commit=commit, poor_count=count,
                                max_error=scalar(record['metrics'][PREFIX + 'palette_error_max'])))
    publications = {}
    for release in sorted({s['release'] for s in samples}):
        path = evidence / f'journal-{release}.jsonl'
        if not path.exists():
            path.write_bytes(ssh(host, shlex.join([
                'journalctl', '-u', f'aerobag-live-feeds-release@{release}.service',
                '--since', (start - timedelta(minutes=30)).isoformat(), '--until', end.isoformat(),
                '--no-pager', '-o', 'json', '-g', 'published nexrad'])))
        rows = []
        for row in read_records(path):
            state = re.search(r'published nexrad (\S+)', row['MESSAGE']).group(1)
            time = int(row['__REALTIME_TIMESTAMP']) / 1_000_000
            rows.append((time, state))
        publications[release] = sorted(rows)
    frames = {}
    for sample in samples:
        rows = publications[sample['release']]
        index = bisect.bisect_right([r[0] for r in rows], timestamp(sample['sampled_at_utc'])) - 1
        assert index >= 0, sample
        published, state = rows[index]
        assert timestamp(sample['sampled_at_utc']) - published < 700, sample
        frame = frames.setdefault(state, dict(state_id=state, release=sample['release'],
                                            commit=sample['commit'], samples=[],
                                            published_at_utc=datetime.fromtimestamp(published, timezone.utc).isoformat()))
        frame['samples'].append(sample)
    requests = []
    for state, frame in frames.items():
        stamp = datetime.strptime(state.split('_')[0], '%Y%m%dT%H%M%SZ')
        frame['source_file'] = stamp.strftime('CONUS_L2_CREF_QCD_%Y%m%d_%H%M%S.tif.gz')
        frame['observed_at_utc'] = stamp.isoformat() + 'Z'
        url = URL + frame['source_file']
        requests.append(dict(state_id=state, url=url, key=hashlib.sha256(url.encode()).hexdigest()))
    remote = '''import json, pathlib
requests = REQUESTS
root = pathlib.Path(CACHE)
out = []
for request in requests:
 path = root / 'http' / (request['key'] + '.json')
 metadata = json.loads(path.read_text()) if path.exists() else None
 out.append(dict(state_id=request['state_id'], metadata=metadata,
                 blob_exists=bool(metadata and (root / 'blobs' / metadata['sha256']).is_file())))
print(json.dumps(out))
'''.replace('REQUESTS', repr(requests)).replace('CACHE', repr(cache))
    cache_path = evidence / 'cache-metadata.json'
    cache_path.write_bytes(ssh(host, 'python3 -', input=remote.encode()))
    sources = evidence / 'sources'
    sources.mkdir(exist_ok=True)
    local_root = args.local_fetch_cache
    local_blobs = {p.name[:16]: p for p in (local_root / 'blobs').glob('*')}
    for row in json.loads(cache_path.read_text()):
        frame = frames[row['state_id']]
        if not row['blob_exists']:
            blob = local_blobs.get(frame['state_id'].split('_')[1])
            if blob:
                target = sources / frame['source_file']
                shutil.copyfile(blob, target)
                sha = hashlib.sha256(target.read_bytes()).hexdigest()
                assert sha == blob.name, blob
                key = hashlib.sha256((URL + frame['source_file']).encode()).hexdigest()
                metadata_path = local_root / 'http' / (key + '.json')
                metadata = json.loads(metadata_path.read_text())
                assert metadata['sha256'] == sha, metadata
                frame['cache_metadata'] = metadata
                frame['recovery'] = 'SHA-256 verified development fetch-cache original'
                print(frame['observed_at_utc'], frame['recovery'], flush=True)
                continue
            frame['recovery'] = 'Source absent from production fetch cache'
            print(frame['observed_at_utc'], frame['recovery'], flush=True)
            continue
        metadata = row['metadata']
        assert metadata['sha256'].startswith(frame['state_id'].split('_')[1]), row
        target = sources / frame['source_file']
        if not target.exists():
            subprocess.run(['scp', f"{host}:{cache}/blobs/{metadata['sha256']}", str(target)], check=True)
        assert hashlib.sha256(target.read_bytes()).hexdigest() == metadata['sha256'], target
        frame['cache_metadata'] = metadata
        frame['recovery'] = 'SHA-256 verified production fetch-cache original'
        print(frame['observed_at_utc'], 'count', frame['samples'][0]['poor_count'],
              'max', frame['samples'][0]['max_error'], 'verified', flush=True)
    for commit in sorted({f['commit'] for f in frames.values()}):
        for name, path in [
            ('tiler.py', 'product/preprocessor/preprocessor-live-feeds/src/nexrad_source_grid_tiles.py'),
            ('palette.json', 'docs/nexrad/analysis/whole-day-greedy-255-palette.json'),
            ('products.rs', 'product/preprocessor/preprocessor-live-feeds/src/products.rs'),
        ]:
            target = evidence / f'{commit}-{name}'
            if not target.exists():
                target.write_bytes(ssh(host, shlex.join(['git', '-C', args.remote_checkout, 'show', f'{commit}:{path}'])))
    (evidence / 'frames.json').write_text(json.dumps(sorted(frames.values(), key=lambda f: f['state_id']), indent=2) + '\n')
    (evidence / 'capture.json').write_text(json.dumps(dict(start=args.start, end=args.end, host=host), indent=2) + '\n')
    print(f'{len(frames)} exact frames, {len(samples)} warning samples', flush=True)


if __name__ == '__main__':
    main()
