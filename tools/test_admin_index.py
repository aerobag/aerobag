#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import unittest

from tools.admin_index import admin_index_html


class AdminIndexTests(unittest.TestCase):
    def test_displays_and_escapes_commit_hash(self) -> None:
        html = admin_index_html(
            title="Aerobag Test",
            front_door="https://example.test",
            commit_hash="abc123<&>",
            cycle_products_root="/artifacts/published",
            live_feed_output_root="/artifacts/live-feeds/v3",
        )

        self.assertIn(
            'Controller commit: <code class="commit">abc123&lt;&amp;&gt;</code>', html
        )

    def test_renders_all_release_roles_from_deployment_health(self) -> None:
        html = admin_index_html(
            title="Aerobag Test",
            front_door="https://example.test",
            commit_hash="abc123",
            cycle_products_root="/artifacts/published",
            live_feed_output_root="/artifacts/live-feeds/v3",
        )

        self.assertIn('fetch("/health.json"', html)
        self.assertIn('card("production"', html)
        self.assertIn('card("staging"', html)
        self.assertIn('card("sunset"', html)
        self.assertIn('/staging/live-feeds/status.html', html)
        self.assertIn('const base = `/releases/${encodeURIComponent(tag)}`', html)
        self.assertIn('live:`${base}/live-feeds/status.html`', html)
        self.assertNotIn('/staging/admin/', html)


if __name__ == "__main__":
    unittest.main()
