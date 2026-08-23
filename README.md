# Aerobag

Aerobag is a flight-planning tool and electronic flight bag. It provides the
same application as a web app for planning on a larger screen and as an Android
app for carrying charts, plates, and other aviation data offline.

The platform interfaces share a Rust application core so navigation behavior,
data contracts, and user-visible policy remain consistent across web and
Android.

The fixture-independent test tier runs in GitHub Actions. See
[Continuous Integration](docs/ci.md) for suite boundaries and local commands.

## Repository Layout

- `crates/` contains shared data formats and geometry libraries.
- `product/preprocessor/` builds cycle data and live-feed products.
- `ui/core-rust/` contains the shared application core and platform bindings.
- `ui/web-app/` contains the web interface.
- `ui/android-app/` contains the Android interface.

## License

Aerobag is free software licensed under the [GNU Affero General Public License,
version 3 or later](LICENSE) (`AGPL-3.0-or-later`).

If you modify Aerobag and make the modified version available for users to
interact with over a network, the license requires you to offer those users the
corresponding source code for the version you are running.

Third-party software and source data retain their respective license and
provenance terms. See [Third-Party Notices](THIRD_PARTY_NOTICES.md).

File licensing metadata follows the
[REUSE specification](https://reuse.software/). Install the repository's pinned
`reuse` version with:

```sh
pipx install "reuse==$(cat .reuse-tool-version)"
```

Then run the same check used by CI:

```sh
./scripts/check-licenses.sh
```
