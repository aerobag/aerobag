# Bootstrap Dependencies

Snapshot date: 2026-04-05
Host context: direct process setup on the 20-core box, not nested container execution

This file records the first successful bootstrap pass so we can convert it into IaC cleanly.

## Install command used

```bash
apt-get update && apt-get install -y \
  ca-certificates \
  exiftool \
  gdal-bin \
  ghostscript \
  imagemagick \
  jq \
  perl \
  python3-bs4 \
  python3-elementpath \
  python3-gdal \
  python3-glob2 \
  python3-numpy \
  python3-pypdf \
  python3-regex \
  python3-tqdm \
  python3-urllib3 \
  sqlite3 \
  unzip \
  zip
```

## Installed package versions

These are the package versions reported after install:

```text
ca-certificates 20240203
gdal-bin 3.8.4+dfsg-3ubuntu3
ghostscript 10.02.1~dfsg1-0ubuntu7.8
imagemagick 8:6.9.12.98+dfsg1-5.2build2
jq 1.7.1-3ubuntu0.24.04.1
perl 5.38.2-3.2ubuntu0.2
python3-bs4 4.12.3-1
python3-elementpath 3.0.2-1
python3-gdal 3.8.4+dfsg-3ubuntu3
python3-glob2 0.5-6
python3-numpy 1:1.26.4+ds-6ubuntu1
python3-pypdf 4.0.2-1
python3-regex 0.1.20221031-2build1
python3-tqdm 4.66.2-2
python3-urllib3 2.0.7-1ubuntu0.6
sqlite3 3.45.1-1ubuntu2.5
unzip 6.0-28ubuntu4.1
zip 3.0-13ubuntu0.2
```

`exiftool` is provided by the `libimage-exiftool-perl` package on Ubuntu 24.04. The CLI reported version `12.76`.

## Tool versions verified

```text
Python 3.12.3
GDAL 3.8.4
Ghostscript 10.02.1
exiftool 12.76
```

## Gaps still likely for IaC

- We have not pinned the apt snapshot yet.
- We have not recorded `python3`, `imagemagick` delegate details, or `convert` policy overrides beyond what the distro ships.
- We may still need extra packages once `data/` joins the capture set.
- If we move back to a dedicated image, this file should become the source of truth for the package layer.
