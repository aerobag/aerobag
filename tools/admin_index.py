from __future__ import annotations

from dataclasses import dataclass
from html import escape


@dataclass(frozen=True)
class AdminLink:
    label: str
    href: str


def admin_index_html(
    *,
    title: str,
    front_door: str,
    cycle_products_root: str,
    live_feed_output_root: str,
) -> str:
    links = [
        AdminLink("Pipeline Health", "/pipeline-health/"),
        AdminLink("Build Watch", "/build-watch/"),
        AdminLink("Live-Feed Status", "/live-feeds/status.html"),
        AdminLink("Health JSON", "/health.json"),
        AdminLink("Current Artifacts JSON", "/packages/current_artifacts.json"),
    ]
    link_items = "\n".join(
        f'    <li><a href="{escape(link.href)}">{escape(link.label)}</a></li>'
        for link in links
    )
    return f"""<!doctype html>
<meta charset="utf-8">
<title>{escape(title)}</title>
<style>
body {{ margin: 32px; font: 15px/1.45 system-ui, sans-serif; color: #17201b; background: #f7f7f4; }}
main {{ max-width: 880px; }}
a {{ color: #075985; }}
code {{ background: #e7e5df; padding: 1px 4px; border-radius: 4px; }}
li {{ margin: 8px 0; }}
</style>
<main>
  <h1>{escape(title)}</h1>
  <p>Front door: <code>{escape(front_door)}</code></p>
  <ul>
{link_items}
  </ul>
  <p>Cycle products are served from <code>{escape(cycle_products_root)}</code>.</p>
  <p>Live-feed output is isolated at <code>{escape(live_feed_output_root)}</code>.</p>
</main>
"""
