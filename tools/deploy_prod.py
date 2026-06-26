#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import shlex
import subprocess
import tempfile
import textwrap
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from admin_index import admin_index_html


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONFIG = REPO_ROOT / "deploy" / "aerobag-prod.json"
BUILD_WATCH_SCRIPT = REPO_ROOT / "product" / "preprocessor" / "scripts" / "watch_build_log.py"
PIPELINE_HEALTH_SCRIPT = (
    REPO_ROOT / "product" / "preprocessor" / "scripts" / "pipeline_health.py"
)
FAA_CYCLE_CALENDAR = REPO_ROOT / "deploy" / "faa-cycle-calendar.json"

SYSTEMD_DIR = "/etc/systemd/system"
NGINX_SITE = "/etc/nginx/sites-available/aerobag.conf"
NGINX_ENABLED_SITE = "/etc/nginx/sites-enabled/aerobag.conf"
ENV_FILE = "/etc/aerobag/env"
DEPLOYED_REV_FILE = "/etc/aerobag/deployed-rev"
DEPLOY_CONFIG_FILE = "/etc/aerobag/deploy-config.json"
REPO_PACKAGE_MANIFEST = "deploy/prod-packages.txt"
BOOTSTRAP_PACKAGES = ["ca-certificates", "git", "rsync"]
CLIENT_DEBUG_LISTEN = "127.0.0.1:8096"
BUILD_WATCH_LISTEN = "127.0.0.1:8097"
PIPELINE_HEALTH_LISTEN = "127.0.0.1:8098"
ANDROID_SDK_ROOT = "/usr/lib/android-sdk"
ANDROID_NDK_VERSION = "26.3.11579264"
LIVE_FEEDS_CONTRACT_PATH = "v2"
ANDROID_SIGNING_EXPECTED_CERT_SHA256 = (
    "09d7edbf70e51b1b6296097876bd39d19b4e71364e82166030228b5674224be1"
)
ANDROID_SIGNING_KEYSTORE_PASSWORD = "android"
ANDROID_SIGNING_KEY_ALIAS = "androiddebugkey"
ANDROID_SIGNING_KEY_PASSWORD = "android"
DEFAULT_ANDROID_SIGNING_SOURCE_KEYSTORE = Path(
    "/root/aerobag-secrets/android/aerobag-app.keystore"
)
DEFAULT_ANDROID_SIGNING_BOOTSTRAP_KEYSTORE = Path("/root/.android/debug.keystore")
DEFAULT_ANDROID_SIGNING_PROD_KEYSTORE = (
    "/etc/aerobag/secrets/android/aerobag-app.keystore"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Deploy Aerobag production from dev to an aerobag-prod container. "
            "The target receives a full git repo via bundle, then builds cycle "
            "publication, live-feeds, the static web app, and Android APK."
        )
    )
    parser.add_argument(
        "--config",
        type=Path,
        default=DEFAULT_CONFIG,
        help=f"deployment config JSON (default: {DEFAULT_CONFIG})",
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="install/update prod but do not start product publication or build web/Android outputs",
    )
    parser.add_argument(
        "--runtime-config-only",
        action="store_true",
        help=(
            "only refresh env, generated helper scripts, nginx, and systemd runtime "
            "config; do not hydrate source, install packages, or touch the product build"
        ),
    )
    parser.add_argument(
        "--allow-dirty",
        action="store_true",
        help="allow deployment when the local checkout has uncommitted changes",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print commands and generated paths without changing prod",
    )
    return parser.parse_args()


def sh_join(args: list[str | os.PathLike[str]]) -> str:
    return " ".join(shlex.quote(os.fspath(arg)) for arg in args)


def shell_quote(value: str | os.PathLike[str]) -> str:
    return shlex.quote(os.fspath(value))


def run_local(
    args: list[str | os.PathLike[str]],
    *,
    cwd: Path = REPO_ROOT,
    input_text: str | None = None,
    capture: bool = False,
    dry_run: bool = False,
) -> subprocess.CompletedProcess[str]:
    print(f"+ cd {cwd} && {sh_join(args)}", flush=True)
    if dry_run:
        return subprocess.CompletedProcess([os.fspath(arg) for arg in args], 0, "")
    return subprocess.run(
        [os.fspath(arg) for arg in args],
        cwd=cwd,
        input=input_text,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
        check=True,
    )


def ssh_target(config: dict[str, Any]) -> str:
    return f"{config['ssh_user']}@{config['ssh_host']}"


def run_ssh(
    config: dict[str, Any],
    command: str,
    *,
    input_text: str | None = None,
    capture: bool = False,
    dry_run: bool = False,
) -> subprocess.CompletedProcess[str]:
    args = ["ssh", "-o", "BatchMode=yes", ssh_target(config), command]
    print(f"+ {sh_join(args)}", flush=True)
    if dry_run:
        return subprocess.CompletedProcess(args, 0, "")
    return subprocess.run(
        args,
        input=input_text,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
        check=True,
    )


def write_remote_file(
    config: dict[str, Any],
    path: str,
    content: str,
    *,
    mode: str = "0644",
    dry_run: bool = False,
) -> None:
    directory = os.path.dirname(path)
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        install -d -m 0755 {shell_quote(directory)}
        tmp="$(mktemp {shell_quote(directory)}/.deploy.XXXXXX)"
        cat > "$tmp"
        chmod {shell_quote(mode)} "$tmp"
        mv "$tmp" {shell_quote(path)}
        """
    ).strip()
    run_ssh(config, command, input_text=content, dry_run=dry_run)


def load_config(path: Path) -> dict[str, Any]:
    config = json.loads(path.read_text(encoding="utf-8"))
    required = [
        "ssh_user",
        "ssh_host",
        "source_root",
        "data_root",
        "artifact_root",
        "ui_target_root",
        "cargo_target_dir",
        "web_dist",
        "checkout_ref",
        "live_feeds_listen",
        "nginx_server_name",
    ]
    missing = [key for key in required if key not in config]
    if missing:
        raise SystemExit(f"{path} missing required keys: {', '.join(missing)}")
    config.setdefault("additional_publication_refs", [])
    config.setdefault("pipeline_health_listen", PIPELINE_HEALTH_LISTEN)
    config.setdefault("pipeline_health_poll_seconds", 60)
    config.setdefault(
        "android_signing_source_keystore",
        os.fspath(DEFAULT_ANDROID_SIGNING_SOURCE_KEYSTORE),
    )
    config.setdefault(
        "android_signing_bootstrap_keystore",
        os.fspath(DEFAULT_ANDROID_SIGNING_BOOTSTRAP_KEYSTORE),
    )
    config.setdefault("android_signing_prod_keystore", DEFAULT_ANDROID_SIGNING_PROD_KEYSTORE)
    config.setdefault(
        "android_signing_expected_cert_sha256",
        ANDROID_SIGNING_EXPECTED_CERT_SHA256,
    )
    config.setdefault(
        "android_signing_keystore_password",
        ANDROID_SIGNING_KEYSTORE_PASSWORD,
    )
    config.setdefault("android_signing_key_alias", ANDROID_SIGNING_KEY_ALIAS)
    config.setdefault("android_signing_key_password", ANDROID_SIGNING_KEY_PASSWORD)
    return config


def publication_refs(config: dict[str, Any]) -> list[str]:
    refs: list[str] = []
    for ref in [*config["additional_publication_refs"], config["checkout_ref"]]:
        if ref not in refs:
            refs.append(ref)
    return refs


def assert_local_refs_exist(config: dict[str, Any], *, dry_run: bool) -> None:
    for ref in publication_refs(config):
        run_local(
            ["git", "rev-parse", "--verify", f"{ref}^{{commit}}"],
            cwd=REPO_ROOT,
            capture=True,
            dry_run=dry_run,
        )


def assert_clean_checkout(*, allow_dirty: bool, dry_run: bool) -> None:
    if allow_dirty:
        return
    result = run_local(
        ["git", "status", "--porcelain"],
        cwd=REPO_ROOT,
        capture=True,
        dry_run=dry_run,
    )
    if result.stdout.strip():
        raise SystemExit(
            "local checkout has uncommitted changes; commit them or pass --allow-dirty"
        )


def local_ref_sha(ref: str, *, dry_run: bool) -> str:
    result = run_local(
        ["git", "rev-parse", f"{ref}^{{commit}}"],
        cwd=REPO_ROOT,
        capture=True,
        dry_run=dry_run,
    )
    return result.stdout.strip() if result.stdout else "<dry-run>"


def normalize_cert_fingerprint(value: str) -> str:
    return "".join(ch for ch in value.lower() if ch in "0123456789abcdef")


def android_keystore_cert_sha256(
    keystore: Path,
    *,
    storepass: str,
    alias: str,
    dry_run: bool,
) -> str:
    if dry_run:
        return ANDROID_SIGNING_EXPECTED_CERT_SHA256
    result = run_local(
        [
            "keytool",
            "-list",
            "-v",
            "-keystore",
            keystore,
            "-storepass",
            storepass,
            "-alias",
            alias,
        ],
        cwd=REPO_ROOT,
        capture=True,
        dry_run=dry_run,
    )
    for line in result.stdout.splitlines():
        if "SHA256:" in line:
            return normalize_cert_fingerprint(line.split("SHA256:", 1)[1])
    raise SystemExit(f"could not find SHA256 fingerprint in {keystore}")


def ensure_local_android_signing_key(config: dict[str, Any], *, dry_run: bool) -> Path:
    source = Path(config["android_signing_source_keystore"]).expanduser()
    bootstrap = Path(config["android_signing_bootstrap_keystore"]).expanduser()
    expected = normalize_cert_fingerprint(config["android_signing_expected_cert_sha256"])
    if not source.exists():
        if not bootstrap.exists():
            if dry_run:
                print(f"+ would install local Android signing key {bootstrap} -> {source}", flush=True)
                return source
            raise SystemExit(
                f"missing Android signing keystore {source}; bootstrap key {bootstrap} also missing"
            )
        print(f"+ install local Android signing key {bootstrap} -> {source}", flush=True)
        if not dry_run:
            source.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
            shutil.copy2(bootstrap, source)
            os.chmod(source, 0o600)
    fingerprint = android_keystore_cert_sha256(
        source,
        storepass=config["android_signing_keystore_password"],
        alias=config["android_signing_key_alias"],
        dry_run=dry_run,
    )
    if fingerprint != expected:
        raise SystemExit(
            f"Android signing keystore {source} has SHA256 {fingerprint}; expected {expected}"
        )
    return source


def install_android_signing_key(config: dict[str, Any], *, dry_run: bool) -> None:
    source = ensure_local_android_signing_key(config, dry_run=dry_run)
    target = config["android_signing_prod_keystore"]
    target_dir = os.path.dirname(target)
    expected = normalize_cert_fingerprint(config["android_signing_expected_cert_sha256"])
    run_ssh(
        config,
        textwrap.dedent(
            f"""
            set -euo pipefail
            install -d -m 0700 {shell_quote(target_dir)}
            """
        ).strip(),
        dry_run=dry_run,
    )
    run_local(
        ["rsync", "-az", "--chmod=F600", source, f"{ssh_target(config)}:{target}"],
        cwd=REPO_ROOT,
        dry_run=dry_run,
    )
    run_ssh(
        config,
        textwrap.dedent(
            f"""
            set -euo pipefail
            key={shell_quote(target)}
            fingerprint="$(keytool -list -v -keystore "$key" -storepass {shell_quote(config["android_signing_keystore_password"])} -alias {shell_quote(config["android_signing_key_alias"])} | awk -F'SHA256: ' '/SHA256:/ {{ gsub(":", "", $2); print tolower($2); exit }}')"
            if [ "$fingerprint" != {shell_quote(expected)} ]; then
              echo "remote Android signing key fingerprint mismatch: $fingerprint" >&2
              exit 1
            fi
            implicit=/root/.android/debug.keystore
            if [ -f "$implicit" ] && ! cmp -s "$implicit" "$key"; then
              quarantine=/root/.android/aerobag-quarantined
              install -d -m 0700 "$quarantine"
              mv "$implicit" "$quarantine/debug.keystore.$(date -u +%Y%m%dT%H%M%SZ)"
            fi
            """
        ).strip(),
        dry_run=dry_run,
    )


def install_bootstrap_packages(config: dict[str, Any], *, dry_run: bool) -> None:
    packages = " ".join(shell_quote(package) for package in BOOTSTRAP_PACKAGES)
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        export DEBIAN_FRONTEND=noninteractive
        apt-get update
        apt-get install -y {packages}
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def install_repo_packages(config: dict[str, Any], *, dry_run: bool) -> None:
    package_file = f"{config['source_root']}/{REPO_PACKAGE_MANIFEST}"
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        export DEBIAN_FRONTEND=noninteractive
        printf 'google-android-installers google-android-installers/mirror select https://dl.google.com\\n' | debconf-set-selections
        package_file={shell_quote(package_file)}
        if [ ! -f "$package_file" ]; then
          echo "missing production package manifest: $package_file" >&2
          exit 1
        fi
        mapfile -t packages < <(grep -Ev '^[[:space:]]*(#|$)' "$package_file")
        apt-get update
        apt-get install -y "${{packages[@]}}"
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def stop_stale_units(config: dict[str, Any], *, dry_run: bool) -> None:
    units = [
        "aerobag-build-fast-subset.timer",
        "aerobag-build-fast-subset.service",
        "aerobag-build-product.timer",
        "aerobag-build-product.service",
        "aerobag-live-feeds.service",
        "aerobag-client-debug-log.service",
        "aerobag-build-watch.service",
        "aerobag-health.timer",
        "aerobag-health.service",
    ]
    unit_args = " ".join(shell_quote(unit) for unit in units)
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        if command -v systemctl >/dev/null 2>&1; then
          systemctl stop {unit_args} 2>/dev/null || true
          systemctl disable aerobag-build-fast-subset.timer aerobag-build-fast-subset.service 2>/dev/null || true
        fi
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def create_git_bundle(bundle_path: Path, *, dry_run: bool) -> None:
    run_local(
        ["git", "bundle", "create", bundle_path, "--all"],
        cwd=REPO_ROOT,
        dry_run=dry_run,
    )


def copy_bundle(config: dict[str, Any], bundle_path: Path, remote_path: str, *, dry_run: bool) -> None:
    target = f"{ssh_target(config)}:{remote_path}"
    run_local(["rsync", "-az", bundle_path, target], cwd=REPO_ROOT, dry_run=dry_run)


def install_repo_from_bundle(
    config: dict[str, Any],
    remote_bundle_path: str,
    *,
    dry_run: bool,
) -> None:
    source_root = config["source_root"]
    checkout_ref = config["checkout_ref"]
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        install -d -m 0755 {shell_quote(os.path.dirname(source_root))}
        install -d -m 0755 /etc/aerobag
        trap 'rm -f {shell_quote(remote_bundle_path)}' EXIT
        if [ ! -d {shell_quote(source_root)}/.git ]; then
          rm -rf {shell_quote(source_root)}
          git clone {shell_quote(remote_bundle_path)} {shell_quote(source_root)}
        fi
        git -C {shell_quote(source_root)} checkout --detach HEAD
        git -C {shell_quote(source_root)} fetch --prune {shell_quote(remote_bundle_path)} \
          '+refs/heads/*:refs/heads/*' \
          '+refs/tags/*:refs/tags/*' \
          '+refs/remotes/*:refs/remotes/*'
        git -C {shell_quote(source_root)} checkout --detach {shell_quote(checkout_ref)}
        git -C {shell_quote(source_root)} reset --hard {shell_quote(checkout_ref)}
        git -C {shell_quote(source_root)} clean -fd
        git -C {shell_quote(source_root)} rev-parse HEAD > {shell_quote(DEPLOYED_REV_FILE)}
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def deploy_config_json(config: dict[str, Any], deployed_rev: str) -> str:
    return (
        json.dumps(
            {
                "deployed_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
                "deployed_rev": deployed_rev,
                "checkout_ref": config["checkout_ref"],
                "publication_refs": publication_refs(config),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )


def env_file(config: dict[str, Any]) -> str:
    artifact_root = config["artifact_root"]
    values = {
        "SOURCE_ROOT": config["source_root"],
        "DATA_ROOT": config["data_root"],
        "ARTIFACT_ROOT": artifact_root,
        "AEROBAG_UI_TARGET_ROOT": config["ui_target_root"],
        "CARGO_TARGET_DIR": config["cargo_target_dir"],
        "AEROBAG_ARTIFACT_WRITE_PATH": artifact_root,
        "AEROBAG_ARTIFACT_READ_PATH": f"{artifact_root}/published",
        "AEROBAG_WEB_DIST": config["web_dist"],
        "AEROBAG_LIVE_FEEDS_LISTEN": config["live_feeds_listen"],
        "AEROBAG_CLIENT_DEBUG_LISTEN": CLIENT_DEBUG_LISTEN,
        "AEROBAG_CLIENT_DEBUG_ROOT": f"{config['data_root']}/client-debug",
        "AEROBAG_BUILD_WATCH_LISTEN": BUILD_WATCH_LISTEN,
        "AEROBAG_BUILD_WATCH_LOG": (
            f"{artifact_root}/logs/orchestrator/published/master.log"
        ),
        "AEROBAG_PIPELINE_HEALTH_LISTEN": config["pipeline_health_listen"],
        "AEROBAG_PIPELINE_HEALTH_POLL_SECONDS": str(config["pipeline_health_poll_seconds"]),
        "ANDROID_HOME": ANDROID_SDK_ROOT,
        "ANDROID_SDK_ROOT": ANDROID_SDK_ROOT,
        "AEROBAG_ANDROID_KEYSTORE": config["android_signing_prod_keystore"],
        "AEROBAG_ANDROID_KEYSTORE_PASSWORD": config["android_signing_keystore_password"],
        "AEROBAG_ANDROID_KEY_ALIAS": config["android_signing_key_alias"],
        "AEROBAG_ANDROID_KEY_PASSWORD": config["android_signing_key_password"],
        "AEROBAG_ANDROID_EXPECTED_CERT_SHA256": config[
            "android_signing_expected_cert_sha256"
        ],
        "PATH": "/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    }
    return "".join(f"{key}={shell_quote(value)}\n" for key, value in values.items())


def public_front_door(config: dict[str, Any]) -> str:
    server_name = config["nginx_server_name"]
    host = config["ssh_host"] if server_name == "_" else server_name
    return f"http://{host}"


def prod_admin_index(config: dict[str, Any]) -> str:
    artifact_root = config["artifact_root"]
    return admin_index_html(
        title="Aerobag Prod",
        front_door=public_front_door(config),
        cycle_products_root=f"{artifact_root}/published",
        live_feed_output_root=f"{artifact_root}/live-feeds/{LIVE_FEEDS_CONTRACT_PATH}",
    )


def ensure_toolchain_script() -> str:
    return r"""#!/usr/bin/env bash
set -euo pipefail
source /etc/aerobag/env
export PATH CARGO_TARGET_DIR AEROBAG_UI_TARGET_ROOT AEROBAG_ARTIFACT_WRITE_PATH AEROBAG_ARTIFACT_READ_PATH

rustup default stable
rustup target add \
  wasm32-unknown-unknown \
  x86_64-linux-android \
  aarch64-linux-android

cd "$SOURCE_ROOT/product/preprocessor"
cargo build --release -p preprocessor-cli -p live-feeds-daemon

WASM_BINDGEN_VERSION="$(python3 - "$SOURCE_ROOT/ui/core-rust/Cargo.lock" <<'PY'
from pathlib import Path
import sys
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

lock = tomllib.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
for package in lock.get("package", []):
    if package.get("name") == "wasm-bindgen":
        print(package["version"])
        raise SystemExit(0)
raise SystemExit("wasm-bindgen version not found in Cargo.lock")
PY
)"

CURRENT_VERSION="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)"
if [ "$CURRENT_VERSION" != "$WASM_BINDGEN_VERSION" ]; then
  cargo install wasm-bindgen-cli --version "$WASM_BINDGEN_VERSION" --locked --force
fi
"""


def ensure_android_sdk_script() -> str:
    return f"""#!/usr/bin/env bash
set -euo pipefail
source /etc/aerobag/env
export PATH ANDROID_HOME ANDROID_SDK_ROOT AEROBAG_UI_TARGET_ROOT

ANDROID_HOME="${{ANDROID_HOME:-{ANDROID_SDK_ROOT}}}"
ANDROID_SDK_ROOT="${{ANDROID_SDK_ROOT:-$ANDROID_HOME}}"
SDKMANAGER="$ANDROID_HOME/cmdline-tools/13.0/bin/sdkmanager"
REQUIRED_NDK="{ANDROID_NDK_VERSION}"

if [ ! -x "$SDKMANAGER" ]; then
  echo "missing Android sdkmanager: $SDKMANAGER" >&2
  exit 1
fi

mkdir -p "$ANDROID_HOME" /root/.android
{{ yes || true; }} | "$SDKMANAGER" --sdk_root="$ANDROID_HOME" --licenses >/dev/null
"$SDKMANAGER" --sdk_root="$ANDROID_HOME" --install \\
  "platforms;android-34" \\
  "build-tools;34.0.0" \\
  "platform-tools" \\
  "ndk;$REQUIRED_NDK"

mkdir -p "$SOURCE_ROOT/ui/android-app"
printf 'sdk.dir=%s\\n' "$ANDROID_HOME" > "$SOURCE_ROOT/ui/android-app/local.properties"

test -d "$ANDROID_HOME/platforms/android-34"
test -d "$ANDROID_HOME/build-tools/34.0.0"
test -x "$ANDROID_HOME/platform-tools/adb"
test -x "$ANDROID_HOME/ndk/$REQUIRED_NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang"
test -x "$ANDROID_HOME/ndk/$REQUIRED_NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang"
"""


def build_product_script(config: dict[str, Any]) -> str:
    refs = " ".join(shell_quote(ref) for ref in publication_refs(config))
    return f"""#!/usr/bin/env bash
set -euo pipefail
source /etc/aerobag/env
export PATH CARGO_TARGET_DIR AEROBAG_UI_TARGET_ROOT AEROBAG_ARTIFACT_WRITE_PATH AEROBAG_ARTIFACT_READ_PATH

mkdir -p "$ARTIFACT_ROOT" "$ARTIFACT_ROOT/cache" "$ARTIFACT_ROOT/published" "$ARTIFACT_ROOT/logs" "$ARTIFACT_ROOT/locks" "$ARTIFACT_ROOT/state" "$ARTIFACT_ROOT/scratch" "$ARTIFACT_ROOT/worktrees" "$AEROBAG_UI_TARGET_ROOT" "$CARGO_TARGET_DIR"

/usr/local/bin/aerobag-ensure-toolchain

cd "$SOURCE_ROOT"
"$SOURCE_ROOT/product/preprocessor/scripts/build_multi_version_publication.py" \\
  --release \\
  --build-root "$ARTIFACT_ROOT" \\
  --target-dir "$CARGO_TARGET_DIR" \\
  {refs}

/usr/local/bin/aerobag-write-health
"""


def build_web_android_script() -> str:
    return """#!/usr/bin/env bash
set -euo pipefail
source /etc/aerobag/env
export PATH CARGO_TARGET_DIR AEROBAG_UI_TARGET_ROOT AEROBAG_ARTIFACT_WRITE_PATH AEROBAG_ARTIFACT_READ_PATH
export ANDROID_HOME ANDROID_SDK_ROOT
export AEROBAG_ANDROID_KEYSTORE AEROBAG_ANDROID_KEYSTORE_PASSWORD AEROBAG_ANDROID_KEY_ALIAS AEROBAG_ANDROID_KEY_PASSWORD AEROBAG_ANDROID_EXPECTED_CERT_SHA256

/usr/local/bin/aerobag-ensure-android-sdk

cd "$SOURCE_ROOT/ui/web-app"
npm run install:wasm-opt
npm run build:release

cd "$SOURCE_ROOT/ui/android-app"
./scripts/build_prod_apk.sh

/usr/local/bin/aerobag-write-health
"""


def health_script() -> str:
    return r"""#!/usr/bin/env python3
from __future__ import annotations

import glob
import json
import os
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path


ENV_FILE = Path("/etc/aerobag/env")
DEPLOYED_REV_FILE = Path("/etc/aerobag/deployed-rev")
DEPLOY_CONFIG_FILE = Path("/etc/aerobag/deploy-config.json")


def read_env() -> dict[str, str]:
    values: dict[str, str] = {}
    if not ENV_FILE.exists():
        return values
    for line in ENV_FILE.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        if len(value) >= 2 and value[0] == "'" and value[-1] == "'":
            value = value[1:-1].replace("'\"'\"'", "'")
        values[key] = value
    return values


def iso_from_mtime(path: Path) -> str | None:
    try:
        return datetime.fromtimestamp(path.stat().st_mtime, timezone.utc).isoformat().replace("+00:00", "Z")
    except FileNotFoundError:
        return None


def age_seconds(path: Path) -> int | None:
    try:
        return max(0, int(time.time() - path.stat().st_mtime))
    except FileNotFoundError:
        return None


def systemctl(*args: str) -> str:
    result = subprocess.run(
        ["systemctl", *args],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.stdout.strip() if result.returncode == 0 else "unknown"


def service_state(unit: str) -> dict[str, str]:
    return {
        "active": systemctl("is-active", unit),
        "enabled": systemctl("is-enabled", unit),
    }


def current_artifacts_summary(path: Path) -> dict[str, object]:
    summary: dict[str, object] = {
        "path": str(path),
        "exists": path.exists(),
        "modified_at_utc": iso_from_mtime(path),
        "age_seconds": age_seconds(path),
    }
    if not path.exists():
        return summary
    try:
        manifests = json.loads(path.read_text(encoding="utf-8"))
        summary["manifest_count"] = len(manifests) if isinstance(manifests, list) else None
        summary["contracts"] = [
            manifest.get("contracts", {})
            for manifest in manifests
            if isinstance(manifest, dict)
        ]
    except Exception as exc:  # noqa: BLE001 - health should report, not crash.
        summary["error"] = str(exc)
    return summary


def latest_build_log(artifact_root: Path) -> dict[str, object] | None:
    candidates = [
        Path(path)
        for path in glob.glob(str(artifact_root / "logs" / "orchestrator" / "**" / "*.log"), recursive=True)
    ]
    existing = [path for path in candidates if path.is_file()]
    if not existing:
        return None
    latest = max(existing, key=lambda path: path.stat().st_mtime)
    return {
        "path": str(latest),
        "modified_at_utc": iso_from_mtime(latest),
        "age_seconds": age_seconds(latest),
    }


def main() -> int:
    env = read_env()
    artifact_root = Path(env.get("ARTIFACT_ROOT", "/mnt/aerobag-data/artifacts"))
    data_root = Path(env.get("DATA_ROOT", "/mnt/aerobag-data"))
    health_root = data_root / "health"
    health_root.mkdir(parents=True, exist_ok=True)

    deploy_config = {}
    if DEPLOY_CONFIG_FILE.exists():
        try:
            deploy_config = json.loads(DEPLOY_CONFIG_FILE.read_text(encoding="utf-8"))
        except Exception as exc:  # noqa: BLE001
            deploy_config = {"error": str(exc)}

    current_path = artifact_root / "published" / "current_artifacts.json"
    live_current = artifact_root / "live-feeds" / "v2" / "current.json"
    payload = {
        "schema_version": 1,
        "generated_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "deployed_rev": DEPLOYED_REV_FILE.read_text(encoding="utf-8").strip()
        if DEPLOYED_REV_FILE.exists()
        else None,
        "deploy_config": deploy_config,
        "services": {
            "aerobag-live-feeds.service": service_state("aerobag-live-feeds.service"),
            "aerobag-client-debug-log.service": service_state("aerobag-client-debug-log.service"),
            "aerobag-build-watch.service": service_state("aerobag-build-watch.service"),
            "aerobag-pipeline-health.service": service_state("aerobag-pipeline-health.service"),
            "aerobag-build-product.service": service_state("aerobag-build-product.service"),
            "aerobag-build-product.timer": service_state("aerobag-build-product.timer"),
            "aerobag-health.timer": service_state("aerobag-health.timer"),
            "nginx.service": service_state("nginx.service"),
        },
        "current_artifacts": current_artifacts_summary(current_path),
        "live_feeds": {
            "current_json": str(live_current),
            "current_json_exists": live_current.exists(),
            "current_json_modified_at_utc": iso_from_mtime(live_current),
            "current_json_age_seconds": age_seconds(live_current),
            "status_url": "/live-feeds/status.json",
        },
        "latest_build_log": latest_build_log(artifact_root),
    }

    target = health_root / "status.json"
    tmp = target.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(target)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
"""


def client_debug_log_script() -> str:
    return r"""#!/usr/bin/env python3
from __future__ import annotations

import json
import os
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def read_env() -> dict[str, str]:
    values: dict[str, str] = {}
    for line in Path("/etc/aerobag/env").read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        if len(value) >= 2 and value[0] == "'" and value[-1] == "'":
            value = value[1:-1].replace("'\"'\"'", "'")
        values[key] = value
    return values


ENV = read_env()
LOG_ROOT = Path(ENV.get("AEROBAG_CLIENT_DEBUG_ROOT", "/mnt/aerobag-data/client-debug"))
LISTEN = ENV.get("AEROBAG_CLIENT_DEBUG_LISTEN", "127.0.0.1:8096")
MAX_BODY_BYTES = 1024 * 1024


def append_records(records: list[object], headers) -> None:
    LOG_ROOT.mkdir(parents=True, exist_ok=True)
    target = LOG_ROOT / f"client-debug-{datetime.now(timezone.utc):%Y%m%d}.jsonl"
    received_at = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    context = {
        "received_at_utc": received_at,
        "remote_addr": headers.get("X-Real-IP") or headers.get("X-Forwarded-For"),
        "user_agent": headers.get("User-Agent"),
        "referer": headers.get("Referer"),
    }
    with target.open("a", encoding="utf-8") as stream:
        for record in records:
            stream.write(json.dumps({**context, "record": record}, sort_keys=True) + "\n")


class Handler(BaseHTTPRequestHandler):
    server_version = "AerobagClientDebug/1"

    def log_message(self, format: str, *args) -> None:
        return

    def do_POST(self) -> None:
        if self.path.split("?", 1)[0] != "/__debug_log":
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            self.send_error(411)
            return
        if length < 0 or length > MAX_BODY_BYTES:
            self.send_error(413)
            return
        try:
            payload = json.loads(self.rfile.read(length).decode("utf-8"))
        except Exception as exc:  # noqa: BLE001
            self.send_error(400, str(exc))
            return
        if not isinstance(payload, list):
            self.send_error(400, "expected JSON list")
            return
        append_records(payload, self.headers)
        self.send_response(204)
        self.end_headers()

    def do_GET(self) -> None:
        if self.path.split("?", 1)[0] == "/__debug_log/health":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"ok":true}\n')
            return
        self.send_error(404)


def main() -> int:
    host, port = LISTEN.rsplit(":", 1)
    server = ThreadingHTTPServer((host, int(port)), Handler)
    print(f"aerobag client debug log listening on {LISTEN}, writing {LOG_ROOT}", flush=True)
    server.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
"""


def build_watch_script() -> str:
    return BUILD_WATCH_SCRIPT.read_text(encoding="utf-8")


def pipeline_health_script() -> str:
    return PIPELINE_HEALTH_SCRIPT.read_text(encoding="utf-8")


def nginx_config(config: dict[str, Any]) -> str:
    web_dist = config["web_dist"]
    published_root = f"{config['artifact_root']}/published"
    health_root = f"{config['data_root']}/health"
    admin_root = f"{config['data_root']}/admin"
    server_name = config["nginx_server_name"]
    icons_root = f"{config['source_root']}/ui/icons"
    return f"""server {{
    listen 80 default_server;
    server_name {server_name};

    root {web_dist};
    index index.html;

    client_max_body_size 256k;

    location = /__debug_log {{
        proxy_pass http://{CLIENT_DEBUG_LISTEN};
        proxy_http_version 1.1;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header User-Agent $http_user_agent;
        proxy_set_header Referer $http_referer;
    }}

    location = /health.json {{
        alias {health_root}/status.json;
        add_header Cache-Control "no-cache";
    }}

    location /build-watch/ {{
        proxy_pass http://{BUILD_WATCH_LISTEN}/;
        proxy_http_version 1.1;
        proxy_buffering off;
    }}

    location /pipeline-health/ {{
        proxy_pass http://{config['pipeline_health_listen']};
        proxy_http_version 1.1;
        proxy_buffering off;
    }}

    location = /admin {{
        return 302 /admin/;
    }}

    location /admin/ {{
        alias {admin_root}/;
        index index.html;
        add_header Cache-Control "no-cache";
    }}

    location = /packages/current_artifacts.json {{
        alias {published_root}/current_artifacts.json;
        add_header Cache-Control "no-cache";
    }}

    location /packages/cache/ {{
        return 404;
    }}

    location ~ ^/packages/(locks|logs|scratch|state|worktrees)/ {{
        return 404;
    }}

    location /packages/ {{
        alias {published_root}/;
        add_header Cache-Control "public, max-age=300";
    }}

    location /icons/ {{
        alias {icons_root}/;
        add_header Cache-Control "public, max-age=3600";
    }}

    location /live-feeds/ {{
        proxy_pass http://{config['live_feeds_listen']};
        proxy_http_version 1.1;
        proxy_buffering off;
        proxy_read_timeout 1h;
    }}

    location / {{
        try_files $uri $uri/ /index.html;
    }}
}}
"""


def build_product_unit() -> str:
    return """[Unit]
Description=Aerobag cycle product publication build
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
EnvironmentFile=/etc/aerobag/env
ExecStart=/usr/local/bin/aerobag-build-product-publication
TimeoutStartSec=infinity
"""


def build_product_timer() -> str:
    return """[Unit]
Description=Run Aerobag product publication builds

[Timer]
OnBootSec=30min
OnUnitActiveSec=2h
RandomizedDelaySec=5min
Persistent=true

[Install]
WantedBy=timers.target
"""


def live_feeds_unit() -> str:
    return """[Unit]
Description=Aerobag live-feeds daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=/etc/aerobag/env
ExecStart=/bin/bash -lc 'source /etc/aerobag/env; exec "$CARGO_TARGET_DIR/release/aerobag-live-feedsd" --live-root "$ARTIFACT_ROOT/live-feeds" --scratch-root "$ARTIFACT_ROOT/scratch/live-feeds" --fetch-cache-root "$ARTIFACT_ROOT/cache/fetch" --fetch-cache-mode fill --listen "$AEROBAG_LIVE_FEEDS_LISTEN"'
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
"""


def client_debug_log_unit() -> str:
    return """[Unit]
Description=Aerobag client debug log receiver
After=network.target

[Service]
Type=simple
EnvironmentFile=/etc/aerobag/env
ExecStart=/usr/local/bin/aerobag-client-debug-log
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"""


def build_watch_unit() -> str:
    return """[Unit]
Description=Aerobag product build web monitor
After=network.target

[Service]
Type=simple
EnvironmentFile=/etc/aerobag/env
ExecStart=/bin/bash -lc 'source /etc/aerobag/env; exec /usr/local/bin/aerobag-build-watch "$AEROBAG_BUILD_WATCH_LOG" --serve "$AEROBAG_BUILD_WATCH_LISTEN" --refresh-seconds 2'
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"""


def pipeline_health_unit() -> str:
    return """[Unit]
Description=Aerobag preprocessing pipeline health monitor
After=network.target aerobag-live-feeds.service aerobag-build-watch.service
Wants=aerobag-live-feeds.service aerobag-build-watch.service

[Service]
Type=simple
EnvironmentFile=/etc/aerobag/env
ExecStart=/bin/bash -lc 'source /etc/aerobag/env; exec /usr/local/bin/aerobag-pipeline-health --calendar /etc/aerobag/faa-cycle-calendar.json --listen "$AEROBAG_PIPELINE_HEALTH_LISTEN" --poll-seconds "$AEROBAG_PIPELINE_HEALTH_POLL_SECONDS"'
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"""


def health_unit() -> str:
    return """[Unit]
Description=Write Aerobag machine-readable health status

[Service]
Type=oneshot
EnvironmentFile=/etc/aerobag/env
ExecStart=/usr/local/bin/aerobag-write-health
"""


def health_timer() -> str:
    return """[Unit]
Description=Refresh Aerobag health status

[Timer]
OnBootSec=1min
OnUnitActiveSec=1min
Persistent=true

[Install]
WantedBy=timers.target
"""


def write_remote_config(
    config: dict[str, Any], *, include_build_config: bool = True, dry_run: bool
) -> None:
    write_remote_file(config, ENV_FILE, env_file(config), dry_run=dry_run)
    write_remote_file(
        config,
        f"{config['data_root']}/admin/index.html",
        prod_admin_index(config),
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        "/usr/local/bin/aerobag-write-health",
        health_script(),
        mode="0755",
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        "/usr/local/bin/aerobag-client-debug-log",
        client_debug_log_script(),
        mode="0755",
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        "/usr/local/bin/aerobag-build-watch",
        build_watch_script(),
        mode="0755",
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        "/usr/local/bin/aerobag-pipeline-health",
        pipeline_health_script(),
        mode="0755",
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        "/etc/aerobag/faa-cycle-calendar.json",
        FAA_CYCLE_CALENDAR.read_text(encoding="utf-8"),
        dry_run=dry_run,
    )
    if include_build_config:
        write_remote_file(
            config,
            "/usr/local/bin/aerobag-ensure-toolchain",
            ensure_toolchain_script(),
            mode="0755",
            dry_run=dry_run,
        )
        write_remote_file(
            config,
            "/usr/local/bin/aerobag-ensure-android-sdk",
            ensure_android_sdk_script(),
            mode="0755",
            dry_run=dry_run,
        )
        write_remote_file(
            config,
            "/usr/local/bin/aerobag-build-product-publication",
            build_product_script(config),
            mode="0755",
            dry_run=dry_run,
        )
        write_remote_file(
            config,
            "/usr/local/bin/aerobag-build-web-and-android",
            build_web_android_script(),
            mode="0755",
            dry_run=dry_run,
        )
    write_remote_file(config, NGINX_SITE, nginx_config(config), dry_run=dry_run)
    if include_build_config:
        write_remote_file(
            config,
            f"{SYSTEMD_DIR}/aerobag-build-product.service",
            build_product_unit(),
            dry_run=dry_run,
        )
        write_remote_file(
            config,
            f"{SYSTEMD_DIR}/aerobag-build-product.timer",
            build_product_timer(),
            dry_run=dry_run,
        )
    write_remote_file(
        config,
        f"{SYSTEMD_DIR}/aerobag-live-feeds.service",
        live_feeds_unit(),
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        f"{SYSTEMD_DIR}/aerobag-client-debug-log.service",
        client_debug_log_unit(),
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        f"{SYSTEMD_DIR}/aerobag-build-watch.service",
        build_watch_unit(),
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        f"{SYSTEMD_DIR}/aerobag-pipeline-health.service",
        pipeline_health_unit(),
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        f"{SYSTEMD_DIR}/aerobag-health.service",
        health_unit(),
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        f"{SYSTEMD_DIR}/aerobag-health.timer",
        health_timer(),
        dry_run=dry_run,
    )


def prepare_remote_paths(config: dict[str, Any], *, dry_run: bool) -> None:
    paths = [
        config["data_root"],
        config["artifact_root"],
        f"{config['artifact_root']}/cache",
        f"{config['artifact_root']}/published",
        f"{config['artifact_root']}/logs",
        f"{config['artifact_root']}/locks",
        f"{config['artifact_root']}/state",
        f"{config['artifact_root']}/scratch",
        f"{config['artifact_root']}/worktrees",
        f"{config['artifact_root']}/live-feeds",
        f"{config['artifact_root']}/live-feeds/{LIVE_FEEDS_CONTRACT_PATH}",
        config["ui_target_root"],
        config["cargo_target_dir"],
        f"{config['data_root']}/admin",
        f"{config['data_root']}/health",
        f"{config['data_root']}/health/pipeline-health",
        f"{config['data_root']}/client-debug",
    ]
    command = "set -euo pipefail\n" + "\n".join(
        f"install -d -m 0755 {shell_quote(path)}" for path in paths
    )
    run_ssh(config, command, dry_run=dry_run)


def reload_services(config: dict[str, Any], *, dry_run: bool) -> None:
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        ln -sfn {shell_quote(NGINX_SITE)} {shell_quote(NGINX_ENABLED_SITE)}
        rm -f /etc/nginx/sites-enabled/default
        nginx -t
        systemctl daemon-reload
        systemctl enable nginx.service
        systemctl restart nginx.service
        systemctl enable aerobag-live-feeds.service aerobag-client-debug-log.service aerobag-build-watch.service aerobag-pipeline-health.service aerobag-build-product.timer aerobag-health.timer
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def start_runtime(config: dict[str, Any], *, skip_build: bool, dry_run: bool) -> None:
    if skip_build:
        command = textwrap.dedent(
            """
            set -euo pipefail
            systemctl restart aerobag-live-feeds.service
            systemctl restart aerobag-client-debug-log.service
            systemctl restart aerobag-build-watch.service
            systemctl restart aerobag-pipeline-health.service
            systemctl start aerobag-health.service || true
            systemctl start aerobag-build-product.timer
            systemctl start aerobag-health.timer
            """
        ).strip()
        run_ssh(config, command, dry_run=dry_run)
        return

    command = textwrap.dedent(
        """
        set -euo pipefail
        systemctl restart aerobag-live-feeds.service
        systemctl restart aerobag-client-debug-log.service
        systemctl restart aerobag-build-watch.service
        systemctl restart aerobag-pipeline-health.service
        systemctl start --no-block aerobag-build-product.service
        systemctl start aerobag-build-product.timer
        systemctl start aerobag-health.timer
        systemctl start aerobag-health.service || true
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def run_initial_toolchain_build(config: dict[str, Any], *, dry_run: bool) -> None:
    run_ssh(config, "/usr/local/bin/aerobag-ensure-toolchain", dry_run=dry_run)


def run_android_sdk_setup(config: dict[str, Any], *, dry_run: bool) -> None:
    run_ssh(config, "/usr/local/bin/aerobag-ensure-android-sdk", dry_run=dry_run)


def run_web_android_build(config: dict[str, Any], *, dry_run: bool) -> None:
    run_ssh(config, "/usr/local/bin/aerobag-build-web-and-android", dry_run=dry_run)


def deploy(config: dict[str, Any], args: argparse.Namespace) -> None:
    if args.runtime_config_only:
        prepare_remote_paths(config, dry_run=args.dry_run)
        write_remote_config(config, include_build_config=False, dry_run=args.dry_run)
        reload_services(config, dry_run=args.dry_run)
        start_runtime(config, skip_build=True, dry_run=args.dry_run)
        return

    assert_local_refs_exist(config, dry_run=args.dry_run)
    assert_clean_checkout(allow_dirty=args.allow_dirty, dry_run=args.dry_run)
    deployed_rev = local_ref_sha(config["checkout_ref"], dry_run=args.dry_run)

    install_bootstrap_packages(config, dry_run=args.dry_run)
    stop_stale_units(config, dry_run=args.dry_run)
    prepare_remote_paths(config, dry_run=args.dry_run)

    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    remote_bundle_path = f"/tmp/aerobag-deploy-{timestamp}.bundle"
    with tempfile.TemporaryDirectory(prefix="aerobag-deploy-") as temp_dir:
        bundle_path = Path(temp_dir) / "aerobag.bundle"
        create_git_bundle(bundle_path, dry_run=args.dry_run)
        copy_bundle(config, bundle_path, remote_bundle_path, dry_run=args.dry_run)
        install_repo_from_bundle(
            config,
            remote_bundle_path,
            dry_run=args.dry_run,
        )

    install_repo_packages(config, dry_run=args.dry_run)
    install_android_signing_key(config, dry_run=args.dry_run)
    write_remote_file(
        config,
        DEPLOY_CONFIG_FILE,
        deploy_config_json(config, deployed_rev),
        dry_run=args.dry_run,
    )
    write_remote_config(config, dry_run=args.dry_run)
    run_initial_toolchain_build(config, dry_run=args.dry_run)
    run_android_sdk_setup(config, dry_run=args.dry_run)
    reload_services(config, dry_run=args.dry_run)
    start_runtime(config, skip_build=args.skip_build, dry_run=args.dry_run)
    if not args.skip_build:
        run_web_android_build(config, dry_run=args.dry_run)


def main() -> int:
    args = parse_args()
    config = load_config(args.config)
    print(
        "deploying refs="
        + ",".join(publication_refs(config))
        + f" to {ssh_target(config)} source={config['source_root']} artifacts={config['artifact_root']}",
        flush=True,
    )
    deploy(config, args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
