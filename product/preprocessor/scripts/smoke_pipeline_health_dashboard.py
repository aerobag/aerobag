#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Exercise dashboard rows and alert navigation in local Chrome, without network inputs."""

import os
from pathlib import Path
import subprocess
import tempfile

import pipeline_health


FIXTURES = """<script>
window.plotCalls = [];
window.Plotly = {
  react: (element) => { if (!element) throw new Error("missing plot"); plotCalls.push(element); },
  purge: () => {},
};
const fixtureMetrics = [
  {id:"channel.production.release.qualification_status", scope:"production", value:"pending", severity:"critical"},
  {id:"channel.release-old.release.qualification_status", scope:"release-old", value:"bypassed", severity:"warning"},
  {id:"channel.production.cycle_product.warning_count", scope:"production", value:154, severity:"warning"},
  {id:"channel.production.cycle_product.weather_camera_site_count", scope:"production", label:"Weather camera sites", value:756, unit:"sites", severity:"warning", warning_threshold:960},
  {id:"input.available", scope:"global", value:true, severity:"ok"},
  {id:"cloud.mode", scope:"global", value:"normal", severity:"ok"},
  {id:"missing.status", scope:"global", value:null, severity:"critical"},
];
const fixtureRecord = {
  sampled_at_utc:"2026-09-07T15:00:00Z",
  sample_age_seconds:0,
  evaluation: {
    top_line_status:"critical",
    metrics:fixtureMetrics,
    alerts:fixtureMetrics.filter(metric => metric.severity !== "ok").map(metric => ({
      metric_id:metric.id, scope:metric.scope, severity:metric.severity, message:String(metric.value),
    })),
    scopes: {
      production:{label:"Production", status:"critical"},
      "release-old":{label:"Sunset old", status:"warning"},
      global:{label:"Global", status:"critical"},
    },
  },
};
window.fetch = async (url) => ({ok:true, json:async () => {
  if (url === "/pipeline-health/current.json") return fixtureRecord;
  if (url === "/pipeline-health/series.json") return {times:[], series:{}, bucket_seconds:300};
  throw new Error(`unexpected request ${url}`);
}});
</script>"""

CHECKS = """<script>
function check(value, message) { if (!value) throw new Error(message); }
const rowFor = (id) => document.getElementById(`metric-${id}`);
const pause = () => new Promise(resolve => setTimeout(resolve, 20));
(async () => {
  await refresh(true);
  clearTimeout(dashboard.refreshTimer);
  check(dashboard.rows.size === fixtureMetrics.length, "every metric must have a row");
  for (const metric of fixtureMetrics) {
    const row = rowFor(metric.id);
    check(row, `missing row ${metric.id}`);
    check(row.querySelector(".pill").textContent === metric.severity, "row severity");
    const expectedValue = formatValue(metric) + (metric.unit ? ` ${metric.unit}` : "");
    check(row.querySelector(".metric-value").textContent === expectedValue, "row value and unit");
    check(Boolean(row.querySelector(".plot")) === (graphValue(metric.value) !== null), "plot eligibility");
    activatePlot(metric.id);
  }
  check(dashboard.plots.size === 3, "only numeric and boolean metrics get plots");
  check(plotCalls.length > 0, "numeric plots are exercised");
  const reusedRow = rowFor(fixtureMetrics[0].id);
  ensureMetricRows(fixtureRecord);
  check(rowFor(fixtureMetrics[0].id) === reusedRow, "unchanged rows are reused");

  // Follow real links and allow the real hashchange handler to apply the scope.
  const links = [...document.querySelectorAll("#alerts a")].map(link => ({id:link.textContent, href:link.getAttribute("href")}));
  for (const link of links) {
    location.hash = "";
    await pause();
    document.querySelector(`#alerts a[href="${CSS.escape(link.href)}"]`).click();
    await pause();
    const metric = fixtureMetrics.find(metric => metric.id === link.id);
    check(selectedScope() === metric.scope, "alert link selects its metric scope");
    check(rowFor(link.id) === document.activeElement, "alert link focuses the visible row");
    check([...dashboard.rows.values()].every(row => row.metric.scope === metric.scope), "scope filters rows");
  }

  // A copied link must work on initial load as well as after clicking.
  dashboard.current = null;
  document.getElementById("metricRows").innerHTML = "";
  dashboard.rowOrder = [];
  await refresh(true);
  check(document.activeElement.id === `metric-${links.at(-1).id}`, "initial deep link focuses its row");

  location.hash = "";
  await pause();
  const changing = fixtureMetrics[0];
  changing.value = 1;
  ensureMetricRows(fixtureRecord);
  check(rowFor(changing.id).querySelector(".plot"), "numeric transition adds plot");
  activatePlot(changing.id);
  changing.value = "passed";
  changing.severity = "ok";
  ensureMetricRows(fixtureRecord);
  check(!rowFor(changing.id).querySelector(".plot"), "status transition removes plot");
  check(!dashboard.plots.has(changing.id), "status transition purges old plot");
  check(rowFor(changing.id).querySelector(".pill").textContent === "ok", "refresh updates status severity");
  clearTimeout(dashboard.refreshTimer);
  document.documentElement.dataset.dashboardSmoke = "PASS";
})().catch(error => {
  document.documentElement.dataset.dashboardSmoke = "FAIL";
  document.body.append(`Dashboard smoke failure: ${error.stack}`);
});
</script>"""


def main() -> None:
    html = pipeline_health.dashboard_html().replace(
        '<script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script>', FIXTURES
    ).replace("</body>", CHECKS + "</body>")
    with tempfile.TemporaryDirectory(prefix="aerobag-health-dashboard-") as temporary:
        root = Path(temporary)
        page = root / "dashboard.html"
        page.write_text(html, encoding="utf-8")
        result = subprocess.run(
            [
                os.environ.get("CHROME_BIN", "google-chrome-stable"),
                "--headless=new", "--no-sandbox", "--disable-gpu",
                "--disable-dev-shm-usage", "--no-first-run",
                f"--user-data-dir={root / 'chrome'}",
                "--virtual-time-budget=2000", "--dump-dom", page.as_uri(),
            ],
            capture_output=True, text=True, timeout=30,
        )
    if result.returncode != 0 or 'data-dashboard-smoke="PASS"' not in result.stdout:
        raise SystemExit(result.stderr + "\n" + result.stdout)
    print("PASS: status rows, plot eligibility, alert links, scopes, deep links, and refreshes")


if __name__ == "__main__":
    main()
