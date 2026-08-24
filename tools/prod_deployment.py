# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Internal production deployment machinery used by prod_manage."""

from __future__ import annotations

import ipaddress
import json
import os
import shlex
import subprocess
import tempfile
import textwrap
import time
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any, Callable

from admin_index import admin_index_html
from live_feed_contract import LIVE_FEEDS_CONTRACT_PATH
from release_reconciler import (
    RECONCILIATION_PROGRESS_RELATIVE_PATH,
    RELEASE_LIVE_FEEDS_STATE_ENV,
    load_desired_releases,
    render_live_feed_nginx_routes,
    resolve_release_tag,
)


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
CARGO_TARGET_PRUNE_SCRIPT = "/usr/local/bin/aerobag-prune-cargo-target"
REPO_PACKAGE_MANIFEST = "deploy/prod-packages.txt"
BOOTSTRAP_PACKAGES = ["ca-certificates", "curl", "git", "rsync"]
CLIENT_DEBUG_LISTEN = "127.0.0.1:8096"
BUILD_WATCH_LISTEN = "127.0.0.1:8097"
PIPELINE_HEALTH_LISTEN = "127.0.0.1:8098"
ANDROID_SDK_ROOT = "/usr/lib/android-sdk"
ANDROID_NDK_VERSION = "26.3.11579264"
ANDROID_SIGNING_EXPECTED_CERT_SHA256 = (
    "09d7edbf70e51b1b6296097876bd39d19b4e71364e82166030228b5674224be1"
)
ANDROID_SIGNING_KEYSTORE_PASSWORD = "android"
ANDROID_SIGNING_KEY_ALIAS = "androiddebugkey"
ANDROID_SIGNING_KEY_PASSWORD = "android"
DEFAULT_ANDROID_SIGNING_SOURCE_KEYSTORE = Path(
    "/root/aerobag-credentials/android/aerobag-app.keystore"
)
DEFAULT_ANDROID_SIGNING_PROD_KEYSTORE = (
    "/etc/aerobag/secrets/android/aerobag-app.keystore"
)
DEFAULT_NMS_NOTAMS_CREDENTIAL_FILE = Path(
    "/root/aerobag-credentials/nms-notams-production.json"
)
DEFAULT_NMS_NOTAMS_PROD_CONFIG = "/etc/aerobag/secrets/nms-notams.json"
NMS_PRODUCTION_API_BASE_URL = "https://api-nms.aim.faa.gov/nmsapi/v1"
NMS_PRODUCTION_TOKEN_URL = "https://api-nms.aim.faa.gov/v1/auth/token"
GOOGLE_CHROME_SIGNING_KEY_URL = "https://dl.google.com/linux/linux_signing_key.pub"
GOOGLE_CHROME_APT_SOURCE = (
    "deb [arch=amd64 signed-by=/etc/apt/keyrings/google-chrome.asc] "
    "https://dl.google.com/linux/chrome/deb/ stable main"
)
COMMAND_LOG_ENV = "AEROBAG_COMMAND_LOG"
ProgressReporter = Callable[[str], None]


def _report(progress: ProgressReporter | None, message: str) -> None:
    if progress is not None:
        progress(message)


def sh_join(args: list[str | os.PathLike[str]]) -> str:
    return " ".join(shlex.quote(os.fspath(arg)) for arg in args)


def shell_quote(value: str | os.PathLike[str]) -> str:
    return shlex.quote(os.fspath(value))


def append_command_log(text: str) -> None:
    path = os.environ.get(COMMAND_LOG_ENV)
    if path is None or not text:
        return
    with open(path, "a", encoding="utf-8") as stream:
        stream.write(text)
        if not text.endswith("\n"):
            stream.write("\n")


def run_command(
    args: list[str],
    *,
    trace: str,
    cwd: Path | None = None,
    input_text: str | None = None,
    capture: bool = False,
    dry_run: bool = False,
) -> subprocess.CompletedProcess[str]:
    command_log = os.environ.get(COMMAND_LOG_ENV)
    if command_log is None:
        print(trace, flush=True)
    else:
        append_command_log(trace)
    if dry_run:
        return subprocess.CompletedProcess(args, 0, "")

    if capture:
        try:
            result = subprocess.run(
                args,
                cwd=cwd,
                input=input_text,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=True,
            )
        except subprocess.CalledProcessError as error:
            append_command_log(error.stdout or "")
            raise
        append_command_log(result.stdout or "")
        return result

    if command_log is not None:
        with open(command_log, "a", encoding="utf-8") as stream:
            return subprocess.run(
                args,
                cwd=cwd,
                input=input_text,
                text=True,
                stdout=stream,
                stderr=subprocess.STDOUT,
                check=True,
            )
    return subprocess.run(
        args,
        cwd=cwd,
        input=input_text,
        text=True,
        check=True,
    )


def run_local(
    args: list[str | os.PathLike[str]],
    *,
    cwd: Path = REPO_ROOT,
    input_text: str | None = None,
    capture: bool = False,
    dry_run: bool = False,
) -> subprocess.CompletedProcess[str]:
    command = [os.fspath(arg) for arg in args]
    return run_command(
        command,
        trace=f"+ cd {cwd} && {sh_join(args)}",
        cwd=cwd,
        input_text=input_text,
        capture=capture,
        dry_run=dry_run,
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
    return run_command(
        args,
        trace=f"+ {sh_join(args)}",
        input_text=input_text,
        capture=capture,
        dry_run=dry_run,
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
        "cargo_target_max_bytes",
        "web_dist",
        "checkout_ref",
        "live_feeds_listen",
        "cloud_server_listen",
        "cloud_server_storage_root",
        "cloud_server_secret_source",
        "cloud_server_secret_target",
        "cloud_server_policy_source",
        "cloud_server_policy_target",
        "nginx_trusted_upstream_proxies",
        "nginx_server_name",
    ]
    missing = [key for key in required if key not in config]
    if missing:
        raise SystemExit(f"{path} missing required keys: {', '.join(missing)}")
    config.setdefault("release_desired_state", "deploy/releases.json")
    config.setdefault("release_live_port_base", 8100)
    config.setdefault("pipeline_health_listen", PIPELINE_HEALTH_LISTEN)
    config.setdefault("pipeline_health_poll_seconds", 60)
    config.setdefault(
        "android_signing_source_keystore",
        os.fspath(DEFAULT_ANDROID_SIGNING_SOURCE_KEYSTORE),
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
    config.setdefault("nms_notams_enabled", False)
    config.setdefault(
        "nms_notams_credential_file",
        os.fspath(DEFAULT_NMS_NOTAMS_CREDENTIAL_FILE),
    )
    config.setdefault("nms_notams_prod_config", DEFAULT_NMS_NOTAMS_PROD_CONFIG)
    validate_build_cache_config(config)
    validate_cloud_deploy_config(config)
    return config


def repo_path(value: str | os.PathLike[str]) -> Path:
    path = Path(value).expanduser()
    return path if path.is_absolute() else REPO_ROOT / path


def cloud_policy(config: dict[str, Any]) -> dict[str, Any]:
    path = repo_path(config["cloud_server_policy_source"])
    try:
        policy = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"invalid ACS runtime policy {path}: {error}") from error
    if not isinstance(policy, dict) or policy.get("schema_version") != 3:
        raise SystemExit(f"ACS runtime policy {path} must use schema_version 3")
    return policy


def validate_build_cache_config(config: dict[str, Any]) -> None:
    data_root = PurePosixPath(config["data_root"])
    cargo_target = PurePosixPath(config["cargo_target_dir"])
    if (
        not data_root.is_absolute()
        or not cargo_target.is_absolute()
        or cargo_target == data_root
        or data_root not in cargo_target.parents
    ):
        raise SystemExit("cargo_target_dir must be a child of data_root")
    maximum = config["cargo_target_max_bytes"]
    if (
        not isinstance(maximum, int)
        or isinstance(maximum, bool)
        or maximum < 1024**3
    ):
        raise SystemExit("cargo_target_max_bytes must be an integer of at least 1 GiB")


def validate_cloud_deploy_config(config: dict[str, Any]) -> None:
    policy = cloud_policy(config)
    request = policy.get("request")
    if not isinstance(request, dict) or not isinstance(request.get("max_body_bytes"), int):
        raise SystemExit("ACS runtime policy is missing request.max_body_bytes")
    proxies = config["nginx_trusted_upstream_proxies"]
    if not isinstance(proxies, list) or not proxies:
        raise SystemExit("nginx_trusted_upstream_proxies must be a non-empty list")
    try:
        for proxy in proxies:
            ipaddress.ip_address(proxy)
    except ValueError as error:
        raise SystemExit(f"invalid nginx trusted upstream proxy: {error}") from error


def publication_refs(config: dict[str, Any]) -> list[str]:
    return list(load_desired_releases(repo_path(config["release_desired_state"])).tags())


def assert_local_refs_exist(config: dict[str, Any], *, dry_run: bool) -> None:
    if dry_run:
        for ref in publication_refs(config):
            run_local(
                ["git", "rev-parse", "--verify", f"{ref}^{{commit}}"],
                cwd=REPO_ROOT,
                capture=True,
                dry_run=True,
            )
        return
    for ref in publication_refs(config):
        resolve_release_tag(REPO_ROOT, ref)


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


def remote_deployed_rev(config: dict[str, Any], *, dry_run: bool) -> str:
    result = run_ssh(
        config,
        f"cat {shell_quote(DEPLOYED_REV_FILE)}",
        capture=True,
        dry_run=dry_run,
    )
    if dry_run:
        return "<dry-run>"
    deployed_rev = result.stdout.strip()
    if not deployed_rev:
        raise RuntimeError(f"empty deployed revision in {DEPLOYED_REV_FILE}")
    return deployed_rev


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
    expected = normalize_cert_fingerprint(config["android_signing_expected_cert_sha256"])
    if not source.exists():
        if dry_run:
            return source
        raise SystemExit(f"missing Android signing keystore {source}")
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
            if [ -f "$implicit" ]; then
              if cmp -s "$implicit" "$key"; then
                rm -f "$implicit"
              else
                quarantine=/root/.android/aerobag-quarantined
                install -d -m 0700 "$quarantine"
                mv "$implicit" "$quarantine/debug.keystore.$(date -u +%Y%m%dT%H%M%SZ)"
              fi
            fi
            """
        ).strip(),
        dry_run=dry_run,
    )


def validate_nms_notams_production_credential(source: Path) -> None:
    try:
        credential = json.loads(source.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"invalid NMS NOTAM credential file {source}: {error}") from error
    if not isinstance(credential, dict):
        raise SystemExit(f"NMS NOTAM credential file must contain a JSON object: {source}")

    expected_values = {
        "sourceEnvironment": "production",
        "apiBaseUrl": NMS_PRODUCTION_API_BASE_URL,
        "tokenUrl": NMS_PRODUCTION_TOKEN_URL,
    }
    for field, expected in expected_values.items():
        if credential.get(field) != expected:
            raise SystemExit(
                f"production deploy requires {field}={expected!r} in {source}"
            )

    for field in ("clientId", "clientSecret"):
        value = credential.get(field)
        if not isinstance(value, str) or not value:
            raise SystemExit(
                f"production deploy requires a non-empty string {field} in {source}"
            )
        if any(character.isspace() or ord(character) < 32 for character in value):
            raise SystemExit(
                f"production deploy rejects whitespace/control characters in {field} in {source}"
            )
    if ":" in credential["clientId"]:
        raise SystemExit(f"production deploy rejects ':' in clientId in {source}")


def install_nms_notams_credential(config: dict[str, Any], *, dry_run: bool) -> None:
    if not config["nms_notams_enabled"]:
        return
    target = config["nms_notams_prod_config"]
    target_dir = os.path.dirname(target)
    source = Path(config["nms_notams_credential_file"]).expanduser()
    if source.is_file():
        validate_nms_notams_production_credential(source)
    elif not dry_run:
        raise SystemExit(f"missing NMS NOTAM credential file: {source}")
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


def install_cloud_server_secret(config: dict[str, Any], *, dry_run: bool) -> None:
    source = Path(config["cloud_server_secret_source"]).expanduser()
    target = config["cloud_server_secret_target"]
    if source.is_file():
        size = source.stat().st_size
        if size != 32:
            raise SystemExit(f"ACS server secret {source} must be exactly 32 bytes, got {size}")
    elif not dry_run:
        raise SystemExit(f"missing ACS production server secret: {source}")
    run_ssh(
        config,
        f"install -d -m 0750 -o root -g aerobag-cloud {shell_quote(os.path.dirname(target))}",
        dry_run=dry_run,
    )
    run_local(
        ["rsync", "-az", "--chmod=F640", source, f"{ssh_target(config)}:{target}"],
        cwd=REPO_ROOT,
        dry_run=dry_run,
    )
    run_ssh(
        config,
        f"chown root:aerobag-cloud {shell_quote(target)} && chmod 0640 {shell_quote(target)}",
        dry_run=dry_run,
    )


def install_cloud_server_policy(config: dict[str, Any], *, dry_run: bool) -> None:
    policy = cloud_policy(config)
    write_remote_file(
        config,
        config["cloud_server_policy_target"],
        json.dumps(policy, indent=2, sort_keys=True) + "\n",
        mode="0644",
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


def install_external_package_sources(config: dict[str, Any], *, dry_run: bool) -> None:
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        install -d -m 0755 /etc/apt/keyrings
        key="$(mktemp)"
        trap 'rm -f "$key"' EXIT
        curl --fail --location --silent --show-error \
          {shell_quote(GOOGLE_CHROME_SIGNING_KEY_URL)} --output "$key"
        install -m 0644 "$key" /etc/apt/keyrings/google-chrome.asc
        printf '%s\n' {shell_quote(GOOGLE_CHROME_APT_SOURCE)} \
          > /etc/apt/sources.list.d/google-chrome.list
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
        "aerobag-cloud-server.service",
        "aerobag-cloud-backup.timer",
        "aerobag-cloud-backup.service",
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


def assert_release_reconciliation_idle(
    config: dict[str, Any], *, dry_run: bool
) -> None:
    """Reject source/config replacement while the release controller is running."""

    lock_path = f"{config['artifact_root']}/locks/release-reconciler.lock"
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        unit=aerobag-build-product.service
        active_state="$(systemctl show "$unit" --property=ActiveState --value 2>/dev/null || true)"
        if [ "$active_state" = active ] || [ "$active_state" = activating ] || [ "$active_state" = reloading ] || [ "$active_state" = deactivating ]; then
          echo "release reconciliation is already running (active_state=$active_state)" >&2
          exit 1
        fi
        install -d -m 0755 {shell_quote(config['artifact_root'] + '/locks')}
        if ! flock -n {shell_quote(lock_path)} true; then
          echo "release reconciliation lock is already held" >&2
          exit 1
        fi
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def quiesce_release_reconciliation(
    config: dict[str, Any], *, dry_run: bool
) -> None:
    """Close the timer race without terminating an active reconciliation."""

    lock_path = f"{config['artifact_root']}/locks/release-reconciler.lock"
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        unit=aerobag-build-product.service
        timer=aerobag-build-product.timer
        timer_was_active=false
        if systemctl is-active --quiet "$timer" 2>/dev/null; then
          timer_was_active=true
        fi
        systemctl stop "$timer" 2>/dev/null || true
        reject() {{
          if [ "$timer_was_active" = true ]; then
            systemctl start "$timer"
          fi
          echo "$1" >&2
          exit 1
        }}
        active_state="$(systemctl show "$unit" --property=ActiveState --value 2>/dev/null || true)"
        if [ "$active_state" = active ] || [ "$active_state" = activating ] || [ "$active_state" = reloading ] || [ "$active_state" = deactivating ]; then
          reject "release reconciliation is already running (active_state=$active_state)"
        fi
        install -d -m 0755 {shell_quote(config['artifact_root'] + '/locks')}
        if ! flock -n {shell_quote(lock_path)} true; then
          reject "release reconciliation lock is already held"
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
        # This is a deployment-owned mirror, never a place to preserve edits.
        git -C {shell_quote(source_root)} reset --hard HEAD
        git -C {shell_quote(source_root)} clean -fd
        git -C {shell_quote(source_root)} fetch --prune {shell_quote(remote_bundle_path)} \
          '+refs/heads/*:refs/heads/*' \
          '+refs/tags/*:refs/tags/*' \
          '+refs/remotes/*:refs/remotes/*'
        git -C {shell_quote(source_root)} checkout --detach --force {shell_quote(checkout_ref)}
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
                "cargo_target_dir": config["cargo_target_dir"],
                "cargo_target_max_bytes": config["cargo_target_max_bytes"],
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
        "AEROBAG_CARGO_TARGET_MAX_BYTES": str(config["cargo_target_max_bytes"]),
        "AEROBAG_ARTIFACT_WRITE_PATH": artifact_root,
        "AEROBAG_ARTIFACT_READ_PATH": f"{artifact_root}/published",
        "AEROBAG_WEB_DIST": config["web_dist"],
        "AEROBAG_LIVE_FEEDS_LISTEN": config["live_feeds_listen"],
        "AEROBAG_CLOUD_SERVER_LISTEN": config["cloud_server_listen"],
        "AEROBAG_CLOUD_SERVER_STORAGE_ROOT": config["cloud_server_storage_root"],
        "AEROBAG_CLOUD_SERVER_SECRET": config["cloud_server_secret_target"],
        "AEROBAG_CLOUD_SERVER_POLICY": config["cloud_server_policy_target"],
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
        "CHROME_BIN": "/usr/bin/google-chrome-stable",
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


def prod_admin_index(config: dict[str, Any], deployed_rev: str) -> str:
    artifact_root = config["artifact_root"]
    return admin_index_html(
        title="Aerobag Prod",
        front_door=public_front_door(config),
        commit_hash=deployed_rev,
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
cargo build --release -p preprocessor-cli

cd "$SOURCE_ROOT/services"
cargo build --release -p aerobag-cloud-server

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


def prune_cargo_target_script() -> str:
    return r"""#!/usr/bin/env bash
set -euo pipefail
source /etc/aerobag/env

: "${DATA_ROOT:?missing DATA_ROOT}"
: "${CARGO_TARGET_DIR:?missing CARGO_TARGET_DIR}"
: "${AEROBAG_CARGO_TARGET_MAX_BYTES:?missing AEROBAG_CARGO_TARGET_MAX_BYTES}"

data_root="$(realpath -m -- "$DATA_ROOT")"
target="$(realpath -m -- "$CARGO_TARGET_DIR")"
case "$target" in
  "$data_root"/*) ;;
  *)
    echo "refusing to prune Cargo target outside DATA_ROOT: $target" >&2
    exit 1
    ;;
esac

if ! [[ "$AEROBAG_CARGO_TARGET_MAX_BYTES" =~ ^[0-9]+$ ]]; then
  echo "invalid AEROBAG_CARGO_TARGET_MAX_BYTES: $AEROBAG_CARGO_TARGET_MAX_BYTES" >&2
  exit 1
fi
if [ ! -d "$target" ]; then
  exit 0
fi

before="$(du -sx --block-size=1 -- "$target" | awk '{print $1}')"
if [ "$before" -le "$AEROBAG_CARGO_TARGET_MAX_BYTES" ]; then
  exit 0
fi

echo "Cargo target uses $before bytes; pruning reusable compiler artifacts"
for profile in debug release; do
  for component in .fingerprint build deps incremental; do
    rm -rf -- "$target/$profile/$component"
  done
done
rm -rf -- "$target/multi-version-binaries"

after="$(du -sx --block-size=1 -- "$target" | awk '{print $1}')"
echo "Cargo target prune complete: $before -> $after bytes"
if [ "$after" -gt "$AEROBAG_CARGO_TARGET_MAX_BYTES" ]; then
  echo "Cargo target remains above its configured limit after pruning" >&2
  exit 1
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
    return f"""#!/usr/bin/env bash
set -euo pipefail
source /etc/aerobag/env
export PATH CHROME_BIN CARGO_TARGET_DIR AEROBAG_UI_TARGET_ROOT AEROBAG_ARTIFACT_WRITE_PATH AEROBAG_ARTIFACT_READ_PATH

mkdir -p "$ARTIFACT_ROOT" "$ARTIFACT_ROOT/cache" "$ARTIFACT_ROOT/published" "$ARTIFACT_ROOT/logs" "$ARTIFACT_ROOT/locks" "$ARTIFACT_ROOT/state" "$ARTIFACT_ROOT/scratch" "$ARTIFACT_ROOT/worktrees" "$ARTIFACT_ROOT/release-builds" "$ARTIFACT_ROOT/channel-generations" "$AEROBAG_UI_TARGET_ROOT" "$CARGO_TARGET_DIR"

PROGRESS_FILE="$ARTIFACT_ROOT/{RECONCILIATION_PROGRESS_RELATIVE_PATH}"
progress_tmp="$PROGRESS_FILE.$$"
printf '%s\n' 'Preparing release tooling' > "$progress_tmp"
mv "$progress_tmp" "$PROGRESS_FILE"

/usr/local/bin/aerobag-ensure-toolchain

# Historical release builds share Cargo's target cache and may replace its
# top-level binary. Pin the controller tool before entering the reconciler.
CONTROLLER_REV="$(git -C "$SOURCE_ROOT" rev-parse HEAD)"
CONTROLLER_TOOL_ROOT="$ARTIFACT_ROOT/release-controller/$CONTROLLER_REV"
mkdir -p "$CONTROLLER_TOOL_ROOT"
install -m 0755 "$CARGO_TARGET_DIR/release/preprocessor-cli" "$CONTROLLER_TOOL_ROOT/preprocessor-cli"

"$SOURCE_ROOT/tools/reconcile_prod_releases.py" \\
  --desired "$SOURCE_ROOT/{config['release_desired_state']}" \\
  --observed "$ARTIFACT_ROOT/state/releases-observed.json" \\
  --source-root "$SOURCE_ROOT" \\
  --artifact-root "$ARTIFACT_ROOT" \\
  --cargo-target-dir "$CARGO_TARGET_DIR" \\
  --controller-preprocessor "$CONTROLLER_TOOL_ROOT/preprocessor-cli" \\
  --ui-target-root "$AEROBAG_UI_TARGET_ROOT" \\
  --live-port-base {config['release_live_port_base']} \\
  --legacy-deployed-rev-file "$ARTIFACT_ROOT/state/legacy-deployed-rev" \\
  --refresh-products

{CARGO_TARGET_PRUNE_SCRIPT}

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

    current_path = artifact_root / "channel-current/production/packages/current_artifacts.json"
    release_state_path = artifact_root / "state/releases-observed.json"
    release_state = {}
    if release_state_path.is_file():
        try:
            release_state = json.loads(release_state_path.read_text(encoding="utf-8"))
        except Exception as exc:  # noqa: BLE001
            release_state = {"error": str(exc)}
    production_release = release_state.get("production")
    live_current = (
        artifact_root
        / "live-feeds/releases"
        / production_release
        / "v3/current.json"
        if isinstance(production_release, str)
        else artifact_root / "live-feeds/v3/current.json"
    )
    release_services = {
        f"aerobag-live-feeds-release@{tag}.service": service_state(
            f"aerobag-live-feeds-release@{tag}.service"
        )
        for tag in release_state.get("releases", {})
    }
    payload = {
        "schema_version": 1,
        "generated_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "deployed_rev": DEPLOYED_REV_FILE.read_text(encoding="utf-8").strip()
        if DEPLOYED_REV_FILE.exists()
        else None,
        "deploy_config": deploy_config,
        "services": {
            **release_services,
            "aerobag-cloud-server.service": service_state("aerobag-cloud-server.service"),
            "aerobag-cloud-backup.service": service_state("aerobag-cloud-backup.service"),
            "aerobag-cloud-backup.timer": service_state("aerobag-cloud-backup.timer"),
            "aerobag-client-debug-log.service": service_state("aerobag-client-debug-log.service"),
            "aerobag-build-watch.service": service_state("aerobag-build-watch.service"),
            "aerobag-pipeline-health.service": service_state("aerobag-pipeline-health.service"),
            "aerobag-build-product.service": service_state("aerobag-build-product.service"),
            "aerobag-build-product.timer": service_state("aerobag-build-product.timer"),
            "aerobag-health.timer": service_state("aerobag-health.timer"),
            "nginx.service": service_state("nginx.service"),
        },
        "current_artifacts": current_artifacts_summary(current_path),
        "releases": release_state,
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
    channel_root = f"{config['artifact_root']}/channel-current"
    health_root = f"{config['data_root']}/health"
    admin_root = f"{config['data_root']}/admin"
    server_name = config["nginx_server_name"]
    icons_root = f"{config['source_root']}/ui/icons"
    trusted_proxies = "\n".join(
        f"    set_real_ip_from {proxy};" for proxy in config["nginx_trusted_upstream_proxies"]
    )
    cloud_body_limit = cloud_policy(config)["request"]["max_body_bytes"]
    return f"""server {{
    listen 80 default_server;
    server_name {server_name};

{trusted_proxies}
    real_ip_header Aerobag-Client-Address;
    real_ip_recursive off;

    root {channel_root}/production/web;
    index index.html;

    client_max_body_size 256k;

    # Live-feed control manifests are ordinary JSON. Payloads already compressed
    # as XZ, ZIP, or PNG use other content types and are not recompressed.
    gzip on;
    gzip_comp_level 6;
    gzip_min_length 256;
    gzip_proxied any;
    gzip_types application/json;
    gzip_vary on;

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
        alias {channel_root}/production/packages/current_artifacts.json;
        add_header Cache-Control "no-cache";
    }}

    location /packages/cache/ {{
        return 404;
    }}

    location ~ ^/packages/(locks|logs|scratch|state|worktrees)/ {{
        return 404;
    }}

    location /packages/ {{
        alias {channel_root}/production/packages/;
        add_header Cache-Control "public, max-age=300";
    }}

    location = /downloads/android-apk.json {{
        alias {channel_root}/production/downloads/android-apk.json;
        add_header Cache-Control "no-cache";
    }}

    location /downloads/ {{
        alias {channel_root}/production/downloads/;
        add_header Cache-Control "public, max-age=300";
    }}

    location = /staging {{
        return 302 /staging/;
    }}

    location /staging/packages/ {{
        alias {channel_root}/staging/packages/;
        add_header Cache-Control "public, max-age=300";
    }}

    location /staging/downloads/ {{
        alias {channel_root}/staging/downloads/;
        add_header Cache-Control "public, max-age=300";
    }}

    # A slash-ended URI runs nginx's index module after alias resolution. Do
    # not alias that URI directly to index.html or nginx appends index.html a
    # second time (index.htmlindex.html).
    location = /staging/ {{
        rewrite ^ /staging/index.html last;
    }}

    location ~ "^/staging/(?:index\\.html|about)$" {{
        alias {channel_root}/staging/web/index.html;
        add_header Cache-Control "no-cache";
    }}

    location /staging/ {{
        alias {channel_root}/staging/web/;
        add_header Cache-Control "public, max-age=300";
    }}

    location ~ "^/releases/([A-Za-z0-9][A-Za-z0-9._-]{{0,79}})/web/$" {{
        rewrite ^/releases/([^/]+)/web/$ /releases/$1/web/index.html last;
    }}

    location ~ "^/releases/([A-Za-z0-9][A-Za-z0-9._-]{{0,79}})/web/(?:index\\.html|about)$" {{
        alias {channel_root}/releases/$1/web/index.html;
        add_header Cache-Control "no-cache";
    }}

    location /releases/ {{
        alias {channel_root}/releases/;
        add_header Cache-Control "public, max-age=300";
    }}

    location /icons/ {{
        alias {icons_root}/;
        add_header Cache-Control "public, max-age=3600";
    }}

    include {channel_root}/*.nginx.conf;

    # EventSource requires its short-lived bearer ticket in the query string.
    # Never copy that transient capability into the host access log.
    location = /cloud/v1/events {{
        access_log off;
        proxy_pass http://{config['cloud_server_listen']};
        proxy_http_version 1.1;
        proxy_set_header Aerobag-Client-Address $remote_addr;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 5m;
    }}

    location = /cloud/v1/status {{
        return 404;
    }}

    location /cloud/ {{
        proxy_pass http://{config['cloud_server_listen']};
        proxy_http_version 1.1;
        proxy_set_header Aerobag-Client-Address $remote_addr;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 5m;
        client_max_body_size {cloud_body_limit};
    }}

    location / {{
        try_files $uri $uri/ /index.html;
    }}
}}
"""


def build_product_unit() -> str:
    return """[Unit]
Description=Aerobag desired-state release reconciliation
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
Description=Reconcile Aerobag releases and refresh product publication

[Timer]
OnBootSec=30min
OnUnitActiveSec=2h
RandomizedDelaySec=5min
Persistent=true

[Install]
WantedBy=timers.target
"""


def release_live_feeds_unit(config: dict[str, Any]) -> str:
    state_root = f"${RELEASE_LIVE_FEEDS_STATE_ENV}"
    # Immutable release binaries currently expose worker-specific state flags.
    # Keep path ownership here: both are children of the one controller-owned
    # live-feed state root, rather than independent deployment settings.
    state_args = (
        ' --tfr-detail-backfill-state-root '
        f'"{state_root}/tfr-detail-backfill"'
    )
    nms_args = ""
    if config["nms_notams_enabled"]:
        nms_args = (
            f" --nms-notams-config {shell_quote(config['nms_notams_prod_config'])}"
            f' --nms-notams-state-root "{state_root}/nms-notams"'
        )
    return f"""[Unit]
Description=Aerobag live-feeds daemon for release %i
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=/etc/aerobag/env
EnvironmentFile=/etc/aerobag/live-feeds/%i.env
ExecStart=/bin/bash -lc 'source /etc/aerobag/env; source /etc/aerobag/live-feeds/%i.env; exec "$AEROBAG_RELEASE_ROOT/bin/aerobag-live-feedsd" --live-root "$AEROBAG_RELEASE_LIVE_ROOT" --scratch-root "$AEROBAG_RELEASE_LIVE_SCRATCH" --fetch-cache-root "$AEROBAG_RELEASE_FETCH_CACHE" --fetch-cache-mode fill --listen "$AEROBAG_RELEASE_LIVE_LISTEN"{state_args}{nms_args}'
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
"""


def cloud_server_unit(config: dict[str, Any]) -> str:
    return f"""[Unit]
Description=Aerobag Cloud Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=aerobag-cloud
Group=aerobag-cloud
EnvironmentFile=/etc/aerobag/env
ExecStart=/bin/bash -lc 'source /etc/aerobag/env; exec "$CARGO_TARGET_DIR/release/aerobag-cloud-serverd" serve --storage-root "$AEROBAG_CLOUD_SERVER_STORAGE_ROOT" --policy "$AEROBAG_CLOUD_SERVER_POLICY" --server-secret "$AEROBAG_CLOUD_SERVER_SECRET" --listen "$AEROBAG_CLOUD_SERVER_LISTEN"'
Restart=always
RestartSec=10
UMask=0077
NoNewPrivileges=true
CapabilityBoundingSet=
PrivateDevices=true
PrivateTmp=true
LockPersonality=true
ProtectControlGroups=true
ProtectClock=true
ProtectHome=true
ProtectHostname=true
ProtectKernelLogs=true
ProtectKernelModules=true
ProtectKernelTunables=true
ProtectProc=invisible
ProtectSystem=strict
ProcSubset=pid
RemoveIPC=true
ReadWritePaths={config['cloud_server_storage_root']}
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
RestrictNamespaces=true
RestrictSUIDSGID=true
SystemCallArchitectures=native
TasksMax=256

[Install]
WantedBy=multi-user.target
"""


def cloud_backup_unit(config: dict[str, Any]) -> str:
    return f"""[Unit]
Description=Snapshot Aerobag Cloud Server storage
After=local-fs.target

[Service]
Type=oneshot
User=aerobag-cloud
Group=aerobag-cloud
EnvironmentFile=/etc/aerobag/env
ExecStart=/bin/bash -lc 'source /etc/aerobag/env; exec "$CARGO_TARGET_DIR/release/aerobag-cloud-serverd" backup-if-due --storage-root "$AEROBAG_CLOUD_SERVER_STORAGE_ROOT" --policy "$AEROBAG_CLOUD_SERVER_POLICY"'
UMask=0077
NoNewPrivileges=true
PrivateDevices=true
PrivateTmp=true
ProtectControlGroups=true
ProtectHome=true
ProtectKernelModules=true
ProtectKernelTunables=true
ProtectSystem=strict
ReadWritePaths={config['cloud_server_storage_root']}
RestrictAddressFamilies=AF_UNIX
RestrictSUIDSGID=true
"""


def cloud_backup_timer(config: dict[str, Any]) -> str:
    cloud_policy(config)
    return """[Unit]
Description=Hourly Aerobag Cloud Server snapshot

[Timer]
OnBootSec=5m
OnUnitActiveSec=15m
RandomizedDelaySec=5m
Unit=aerobag-cloud-backup.service

[Install]
WantedBy=timers.target
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
After=network.target aerobag-cloud-server.service aerobag-build-watch.service
Wants=aerobag-cloud-server.service aerobag-build-watch.service

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
    config: dict[str, Any],
    *,
    deployed_rev: str,
    include_build_config: bool = True,
    dry_run: bool,
) -> None:
    write_remote_file(config, ENV_FILE, env_file(config), dry_run=dry_run)
    write_remote_file(
        config,
        f"{config['data_root']}/admin/index.html",
        prod_admin_index(config, deployed_rev),
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
            CARGO_TARGET_PRUNE_SCRIPT,
            prune_cargo_target_script(),
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
        f"{SYSTEMD_DIR}/aerobag-live-feeds-release@.service",
        release_live_feeds_unit(config),
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        f"{SYSTEMD_DIR}/aerobag-cloud-server.service",
        cloud_server_unit(config),
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        f"{SYSTEMD_DIR}/aerobag-cloud-backup.service",
        cloud_backup_unit(config),
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        f"{SYSTEMD_DIR}/aerobag-cloud-backup.timer",
        cloud_backup_timer(config),
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
    command = (
        "set -euo pipefail\n"
        "if ! id -u aerobag-cloud >/dev/null 2>&1; then "
        "useradd --system --home-dir /nonexistent --shell /usr/sbin/nologin aerobag-cloud; fi\n"
        + "\n".join(f"install -d -m 0755 {shell_quote(path)}" for path in paths)
        + "\n"
        + f"install -d -m 0700 -o aerobag-cloud -g aerobag-cloud {shell_quote(config['cloud_server_storage_root'])}"
    )
    run_ssh(config, command, dry_run=dry_run)


def ensure_legacy_channel_view(config: dict[str, Any], *, dry_run: bool) -> None:
    """Keep the pre-reconciler deployment live during the first isolated build."""

    artifact_root = config["artifact_root"]
    generation = f"{artifact_root}/channel-generations/legacy-bootstrap"
    write_remote_file(
        config,
        f"{generation}/gc-root-manifests.json",
        json.dumps(
            {
                "schema_version": 1,
                "current_artifacts_paths": [
                    "production/packages/current_artifacts.json"
                ],
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        dry_run=dry_run,
    )
    write_remote_file(
        config,
        f"{generation}/live-feeds.nginx.conf",
        render_live_feed_nginx_routes(
            production_endpoint=f"http://{config['live_feeds_listen']}",
            staging_endpoint=None,
            release_endpoints={},
        ),
        dry_run=dry_run,
    )
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        generation={shell_quote(generation)}
        install -d -m 0755 "$generation/production" "$generation/releases"
        ln -sfn {shell_quote(config['web_dist'])} "$generation/production/web"
        ln -sfn {shell_quote(artifact_root + '/published')} "$generation/production/packages"
        ln -sfn {shell_quote(config['web_dist'] + '/downloads')} "$generation/production/downloads"
        if [ ! -e {shell_quote(artifact_root + '/channel-current')} ]; then
          if [ ! -s {shell_quote(DEPLOYED_REV_FILE)} ]; then
            echo "cannot bootstrap release channels without the deployed revision" >&2
            exit 1
          fi
          install -m 0644 {shell_quote(DEPLOYED_REV_FILE)} {shell_quote(artifact_root + '/state/legacy-deployed-rev')}
          ln -s channel-generations/legacy-bootstrap {shell_quote(artifact_root + '/channel-current')}
        fi
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def migrate_cloud_storage_layout(config: dict[str, Any], *, dry_run: bool) -> None:
    legacy_root = f"{config['data_root']}/cloud"
    storage_root = config["cloud_server_storage_root"]
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        legacy={shell_quote(legacy_root)}
        storage={shell_quote(storage_root)}
        if [ -f "$legacy/cloud.sqlite3" ]; then
          if [ -e "$storage/live/cloud.sqlite3" ]; then
            echo "both legacy and current ACS databases exist" >&2
            exit 1
          fi
          systemctl stop aerobag-cloud-server.service 2>/dev/null || true
          install -d -m 0700 -o aerobag-cloud -g aerobag-cloud "$storage/live"
          mv "$legacy/cloud.sqlite3" "$storage/live/cloud.sqlite3"
          for suffix in -wal -shm; do
            if [ -e "$legacy/cloud.sqlite3$suffix" ]; then
              mv "$legacy/cloud.sqlite3$suffix" "$storage/live/cloud.sqlite3$suffix"
            fi
          done
          if [ -d "$legacy/blobs" ]; then
            mv "$legacy/blobs" "$storage/live/blobs"
          fi
          chown -R aerobag-cloud:aerobag-cloud "$storage"
        fi
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def reload_services(config: dict[str, Any], *, dry_run: bool) -> None:
    command = textwrap.dedent(
        f"""
        set -euo pipefail
        ln -sfn {shell_quote(NGINX_SITE)} {shell_quote(NGINX_ENABLED_SITE)}
        rm -f /etc/nginx/sites-enabled/default
        nginx -t
        systemctl daemon-reload
        systemctl enable --now nginx.service
        systemctl reload nginx.service
        systemctl enable aerobag-cloud-server.service aerobag-cloud-backup.timer aerobag-client-debug-log.service aerobag-build-watch.service aerobag-pipeline-health.service aerobag-build-product.timer aerobag-health.timer
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def start_support_runtime(config: dict[str, Any], *, dry_run: bool) -> None:
    command = textwrap.dedent(
        """
        set -euo pipefail
        systemctl restart aerobag-cloud-server.service
        systemctl restart aerobag-client-debug-log.service
        systemctl restart aerobag-build-watch.service
        systemctl restart aerobag-pipeline-health.service
        systemctl start aerobag-health.service || true
        systemctl start aerobag-build-product.timer
        systemctl start aerobag-health.timer
        systemctl start aerobag-cloud-backup.timer
        """
    ).strip()
    run_ssh(config, command, dry_run=dry_run)


def run_release_reconciliation(
    config: dict[str, Any],
    *,
    progress: ProgressReporter | None = None,
    dry_run: bool,
) -> None:
    """Start the controller and relay only its human-scale progress marker."""

    unit = "aerobag-build-product.service"
    progress_path = (
        Path(config["artifact_root"]) / RECONCILIATION_PROGRESS_RELATIVE_PATH
    )
    start_command = textwrap.dedent(
        f"""
        set -euo pipefail
        rm -f {shell_quote(progress_path)}
        systemctl reset-failed {unit} || true
        systemctl start --no-block {unit}
        """
    ).strip()
    run_ssh(config, start_command, dry_run=dry_run)
    if dry_run:
        return

    last_progress = None
    while True:
        status_command = textwrap.dedent(
            f"""
            set -euo pipefail
            active_state="$(systemctl show {unit} --property=ActiveState --value)"
            result="$(systemctl show {unit} --property=Result --value)"
            progress=''
            if test -s {shell_quote(progress_path)}; then
              progress="$(cat {shell_quote(progress_path)})"
            fi
            printf '%s\t%s\t%s\n' "$active_state" "$result" "$progress"
            """
        ).strip()
        status = run_ssh(config, status_command, capture=True, dry_run=False)
        try:
            active_state, result, current_progress = status.stdout.rstrip("\n").split(
                "\t", 2
            )
        except ValueError as error:
            raise RuntimeError(
                f"invalid release reconciliation status: {status.stdout!r}"
            ) from error
        if current_progress and current_progress != last_progress:
            _report(progress, current_progress)
            last_progress = current_progress
        if active_state in {"active", "activating", "reloading", "deactivating"}:
            time.sleep(1)
            continue
        if active_state == "inactive" and result == "success":
            return

        failure_command = textwrap.dedent(
            f"""
            systemctl status {unit} --no-pager >&2 || true
            journalctl -u {unit} -n 100 --no-pager >&2 || true
            exit 1
            """
        ).strip()
        run_ssh(config, failure_command, dry_run=False)
        raise AssertionError("failed service diagnostics unexpectedly returned")


def start_reconciled_runtime(
    config: dict[str, Any],
    *,
    progress: ProgressReporter | None = None,
    dry_run: bool,
) -> None:
    restart_command = textwrap.dedent(
        """
        set -euo pipefail
        systemctl restart aerobag-cloud-server.service
        systemctl restart aerobag-client-debug-log.service
        systemctl restart aerobag-build-watch.service
        systemctl restart aerobag-pipeline-health.service
        """
    ).strip()
    run_ssh(config, restart_command, dry_run=dry_run)
    run_release_reconciliation(config, progress=progress, dry_run=dry_run)
    finish_command = textwrap.dedent(
        """
        set -euo pipefail
        systemctl start aerobag-build-product.timer
        systemctl start aerobag-health.timer
        systemctl start aerobag-cloud-backup.timer
        systemctl start aerobag-health.service || true
        """
    ).strip()
    run_ssh(config, finish_command, dry_run=dry_run)


def start_release_live_feeds(config: dict[str, Any], *, dry_run: bool) -> None:
    units = [
        f"aerobag-live-feeds-release@{tag}.service"
        for tag in publication_refs(config)
    ]
    if not units:
        return
    command = "systemctl start " + " ".join(shell_quote(unit) for unit in units)
    run_ssh(config, command, dry_run=dry_run)


def run_initial_toolchain_build(config: dict[str, Any], *, dry_run: bool) -> None:
    run_ssh(config, "/usr/local/bin/aerobag-ensure-toolchain", dry_run=dry_run)


def run_android_sdk_setup(config: dict[str, Any], *, dry_run: bool) -> None:
    run_ssh(config, "/usr/local/bin/aerobag-ensure-android-sdk", dry_run=dry_run)


def sync_source_checkout(config: dict[str, Any], *, dry_run: bool) -> None:
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    remote_bundle_path = f"/tmp/aerobag-deploy-{timestamp}.bundle"
    with tempfile.TemporaryDirectory(prefix="aerobag-deploy-") as temp_dir:
        bundle_path = Path(temp_dir) / "aerobag.bundle"
        create_git_bundle(bundle_path, dry_run=dry_run)
        copy_bundle(config, bundle_path, remote_bundle_path, dry_run=dry_run)
        install_repo_from_bundle(
            config,
            remote_bundle_path,
            dry_run=dry_run,
        )


def repair_runtime(
    config: dict[str, Any],
    *,
    progress: ProgressReporter | None = None,
    dry_run: bool = False,
) -> None:
    """Repair generated runtime configuration without hydrating build tools."""

    _report(progress, "Repairing generated runtime configuration")
    deployed_rev = remote_deployed_rev(config, dry_run=dry_run)
    prepare_remote_paths(config, dry_run=dry_run)
    ensure_legacy_channel_view(config, dry_run=dry_run)
    migrate_cloud_storage_layout(config, dry_run=dry_run)
    install_nms_notams_credential(config, dry_run=dry_run)
    install_cloud_server_secret(config, dry_run=dry_run)
    install_cloud_server_policy(config, dry_run=dry_run)
    write_remote_config(
        config,
        deployed_rev=deployed_rev,
        include_build_config=False,
        dry_run=dry_run,
    )
    reload_services(config, dry_run=dry_run)
    start_support_runtime(config, dry_run=dry_run)
    start_release_live_feeds(config, dry_run=dry_run)


def reconcile_host(
    config: dict[str, Any],
    *,
    progress: ProgressReporter | None = None,
    dry_run: bool = False,
) -> None:
    """Converge an empty, damaged, or stale production host from desired state."""

    _report(progress, "Checking host prerequisites and persistent paths")
    assert_local_refs_exist(config, dry_run=dry_run)
    assert_clean_checkout(allow_dirty=False, dry_run=dry_run)
    deployed_rev = local_ref_sha(config["checkout_ref"], dry_run=dry_run)

    install_bootstrap_packages(config, dry_run=dry_run)
    quiesce_release_reconciliation(config, dry_run=dry_run)
    prepare_remote_paths(config, dry_run=dry_run)
    ensure_legacy_channel_view(config, dry_run=dry_run)
    migrate_cloud_storage_layout(config, dry_run=dry_run)
    _report(progress, "Synchronizing the production controller checkout")
    sync_source_checkout(config, dry_run=dry_run)

    _report(progress, "Installing runtime dependencies and configuration")
    install_external_package_sources(config, dry_run=dry_run)
    install_repo_packages(config, dry_run=dry_run)
    install_android_signing_key(config, dry_run=dry_run)
    install_nms_notams_credential(config, dry_run=dry_run)
    install_cloud_server_secret(config, dry_run=dry_run)
    install_cloud_server_policy(config, dry_run=dry_run)
    write_remote_file(
        config,
        DEPLOY_CONFIG_FILE,
        deploy_config_json(config, deployed_rev),
        dry_run=dry_run,
    )
    write_remote_config(config, deployed_rev=deployed_rev, dry_run=dry_run)
    run_initial_toolchain_build(config, dry_run=dry_run)
    run_android_sdk_setup(config, dry_run=dry_run)
    # Keep the old runtime serving until every fallible installation step has
    # completed. Only the final service handoff needs these units stopped.
    stop_stale_units(config, dry_run=dry_run)
    reload_services(config, dry_run=dry_run)
    _report(progress, "Reconciling release artifacts and channel assignments")
    start_reconciled_runtime(config, progress=progress, dry_run=dry_run)


def activate_release_intent(
    config: dict[str, Any],
    *,
    progress: ProgressReporter | None = None,
    dry_run: bool = False,
) -> None:
    """Publish desired-state changes and run only the release controller."""

    _report(progress, "Synchronizing promoted release intent")
    assert_local_refs_exist(config, dry_run=dry_run)
    assert_clean_checkout(allow_dirty=False, dry_run=dry_run)
    quiesce_release_reconciliation(config, dry_run=dry_run)
    try:
        sync_source_checkout(config, dry_run=dry_run)
        _report(progress, "Switching the production channel")
        run_release_reconciliation(config, progress=progress, dry_run=dry_run)
    finally:
        run_ssh(
            config,
            "systemctl start aerobag-build-product.timer",
            dry_run=dry_run,
        )


def failed_command_summary(command: str | list[str]) -> str:
    parts = [command] if isinstance(command, str) else [str(part) for part in command]
    if parts and Path(parts[0]).name == "ssh":
        target = next((part for part in parts[1:] if "@" in part), "remote host")
        return f"remote command on {target}"
    concise = [part for part in parts if "\n" not in part]
    return " ".join(concise) or Path(parts[0]).name
