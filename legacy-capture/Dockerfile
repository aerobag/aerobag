FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        exiftool \
        gdal-bin \
        ghostscript \
        imagemagick \
        jq \
        perl \
        python3 \
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
        zip && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /work

CMD ["/bin/bash"]
