#!/usr/bin/env python3

import os
import sys

sys.path.insert(0, os.getcwd())

import common
import cycle


def main() -> int:
    start_date = cycle.get_version_start(cycle.get_cycle_download())
    all_charts = [
        "https://nfdc.faa.gov/webContent/28DaySub/28DaySubscription_Effective_" + start_date + ".zip",
        "https://nfdc.faa.gov/webContent/28DaySub/" + start_date + "/aixm5.0.zip",
        "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP",
        "https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_" + start_date[2:].replace("-", "") + ".zip",
    ]

    common.download_list(all_charts)
    common.call_script("cp legacy/* .")
    common.make_data()
    common.make_db()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
