#!/usr/bin/env python3
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
            live_feed_output_root="/artifacts/live-feeds/v2",
        )

        self.assertIn('Commit: <code class="commit">abc123&lt;&amp;&gt;</code>', html)


if __name__ == "__main__":
    unittest.main()
