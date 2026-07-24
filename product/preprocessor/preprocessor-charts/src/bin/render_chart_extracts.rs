// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{env, path::PathBuf};

use anyhow::{bail, Context};
use preprocessor_charts::{build_family_extracts, ChartExtractKind};
use preprocessor_core::ChartFamily;

fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1);
    let kind = match args.next().as_deref() {
        Some("legend") => ChartExtractKind::Legend,
        Some("inset") => ChartExtractKind::Inset,
        Some(other) => bail!("unsupported chart extract type {other:?}"),
        None => bail!(
            "usage: render_chart_extracts <legend|inset> <sec|tac|flyway|enr-l|enr-h> <chart-work-dir>"
        ),
    };
    let family = match args.next().as_deref() {
        Some("sec") => ChartFamily::Sec,
        Some("tac") => ChartFamily::Tac,
        Some("flyway") => ChartFamily::Flyway,
        Some("enr-l") => ChartFamily::EnrL,
        Some("enr-h") => ChartFamily::EnrH,
        Some(other) => bail!("unsupported chart family {other:?}"),
        None => bail!("missing chart family"),
    };
    let work_dir = PathBuf::from(args.next().context("missing chart work directory")?);
    if args.next().is_some() {
        bail!("unexpected extra argument");
    }
    let result = build_family_extracts(family, &work_dir, kind)?;
    for output in result.output_paths {
        println!("{}", output.display());
    }
    Ok(())
}
