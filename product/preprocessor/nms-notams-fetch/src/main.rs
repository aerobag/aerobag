use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context};
use nms_notams_fetch::collector::{run_collector, CollectorOptions, NmsApiCollectorStore};
use nms_notams_fetch::fixture::capture_nms_fixture;
use nms_notams_fetch::{capture_initial_load, NmsClient, NmsConfig};
use preprocessor_live_feeds::nms_initial_load::parse_nms_initial_load;
use preprocessor_live_feeds::nms_initial_load::NmsNotamClassification;

fn main() -> anyhow::Result<()> {
    match Command::parse(std::env::args().skip(1))? {
        Command::Fetch(arguments) => {
            let config = NmsConfig::from_path(&arguments.config_path)?;
            let mut client = NmsClient::new(config);
            capture_initial_load(
                &arguments.output_dir,
                &arguments.classifications,
                &mut client,
            )?;
            println!("{}", arguments.output_dir.display());
        }
        Command::Collect(arguments) => {
            let config = NmsConfig::from_path(&arguments.config_path)?;
            let mut client = NmsClient::new(config);
            run_collector(
                &NmsApiCollectorStore::new(&arguments.state_root),
                &mut client,
                &CollectorOptions {
                    poll_interval: Duration::from_secs(arguments.poll_seconds),
                    overlap: Duration::from_secs(arguments.overlap_seconds),
                    run_duration: arguments.duration_seconds.map(Duration::from_secs),
                    max_polls: arguments.max_polls,
                },
            )?;
        }
        Command::Inspect(arguments) => {
            let input = File::open(&arguments.input_path)
                .with_context(|| format!("failed to open {}", arguments.input_path.display()))?;
            let parsed = parse_nms_initial_load(BufReader::new(input), arguments.classification)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "classification": parsed.classification,
                    "feature_collection_timestamp": parsed.feature_collection_timestamp,
                    "declared_record_count": parsed.declared_record_count,
                    "parsed_message_count": parsed.parsed_message_count,
                    "canonical_record_count": parsed.records.len(),
                    "duplicate_record_ids": parsed.duplicate_record_ids,
                    "rejections": parsed.rejections,
                }))?
            );
        }
        Command::CaptureFixture(arguments) => {
            let manifest = capture_nms_fixture(
                &arguments.initial_load_dir,
                &arguments.state_root,
                &arguments.output_dir,
                &arguments.captured_by_commit,
            )?;
            println!("{}", manifest.display());
        }
    }
    Ok(())
}

enum Command {
    Fetch(FetchArguments),
    Collect(CollectArguments),
    Inspect(InspectArguments),
    CaptureFixture(CaptureFixtureArguments),
}

impl Command {
    fn parse(mut arguments: impl Iterator<Item = String>) -> anyhow::Result<Self> {
        match arguments.next().as_deref() {
            Some("fetch") => Ok(Self::Fetch(FetchArguments::parse(arguments)?)),
            Some("collect") => Ok(Self::Collect(CollectArguments::parse(arguments)?)),
            Some("inspect") => Ok(Self::Inspect(InspectArguments::parse(arguments)?)),
            Some("capture-fixture") => Ok(Self::CaptureFixture(CaptureFixtureArguments::parse(
                arguments,
            )?)),
            Some("--help" | "-h") => {
                print_usage();
                std::process::exit(0);
            }
            Some(command) => bail!("unknown command {command}"),
            None => bail!("a command is required; use --help for usage"),
        }
    }
}

struct CaptureFixtureArguments {
    initial_load_dir: PathBuf,
    state_root: PathBuf,
    output_dir: PathBuf,
    captured_by_commit: String,
}

impl CaptureFixtureArguments {
    fn parse(arguments: impl Iterator<Item = String>) -> anyhow::Result<Self> {
        let mut initial_load_dir = None;
        let mut state_root = None;
        let mut output_dir = None;
        let mut captured_by_commit = None;
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--initial-load" => {
                    initial_load_dir = Some(PathBuf::from(
                        arguments.next().context("--initial-load requires a path")?,
                    ));
                }
                "--state-root" => {
                    state_root = Some(PathBuf::from(
                        arguments.next().context("--state-root requires a path")?,
                    ));
                }
                "--output" => {
                    output_dir = Some(PathBuf::from(
                        arguments.next().context("--output requires a path")?,
                    ));
                }
                "--captured-by-commit" => {
                    captured_by_commit = Some(
                        arguments
                            .next()
                            .context("--captured-by-commit requires an object ID")?,
                    );
                }
                _ => bail!("unknown argument {argument}"),
            }
        }
        Ok(Self {
            initial_load_dir: initial_load_dir.context("--initial-load is required")?,
            state_root: state_root.context("--state-root is required")?,
            output_dir: output_dir.context("--output is required")?,
            captured_by_commit: captured_by_commit.context("--captured-by-commit is required")?,
        })
    }
}

struct CollectArguments {
    config_path: PathBuf,
    state_root: PathBuf,
    poll_seconds: u64,
    overlap_seconds: u64,
    duration_seconds: Option<u64>,
    max_polls: Option<usize>,
}

impl CollectArguments {
    fn parse(arguments: impl Iterator<Item = String>) -> anyhow::Result<Self> {
        let mut config_path = None;
        let mut state_root = None;
        let mut poll_seconds = 180u64;
        let mut overlap_seconds = 600u64;
        let mut duration_seconds = None;
        let mut max_polls = None;
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--config" => {
                    config_path = Some(PathBuf::from(
                        arguments.next().context("--config requires a path")?,
                    ));
                }
                "--state-root" => {
                    state_root = Some(PathBuf::from(
                        arguments.next().context("--state-root requires a path")?,
                    ));
                }
                "--poll-seconds" => {
                    poll_seconds = parse_positive_u64(
                        "--poll-seconds",
                        &arguments
                            .next()
                            .context("--poll-seconds requires a value")?,
                    )?;
                }
                "--overlap-seconds" => {
                    overlap_seconds = arguments
                        .next()
                        .context("--overlap-seconds requires a value")?
                        .parse()
                        .context("--overlap-seconds must be an integer")?;
                }
                "--duration-seconds" => {
                    duration_seconds = Some(parse_positive_u64(
                        "--duration-seconds",
                        &arguments
                            .next()
                            .context("--duration-seconds requires a value")?,
                    )?);
                }
                "--max-polls" => {
                    max_polls = Some(
                        arguments
                            .next()
                            .context("--max-polls requires a value")?
                            .parse::<usize>()
                            .context("--max-polls must be an integer")?,
                    );
                    if max_polls == Some(0) {
                        bail!("--max-polls must be greater than zero");
                    }
                }
                _ => bail!("unknown argument {argument}"),
            }
        }
        Ok(Self {
            config_path: config_path.context("--config is required")?,
            state_root: state_root.context("--state-root is required")?,
            poll_seconds,
            overlap_seconds,
            duration_seconds,
            max_polls,
        })
    }
}

struct InspectArguments {
    input_path: PathBuf,
    classification: NmsNotamClassification,
}

impl InspectArguments {
    fn parse(arguments: impl Iterator<Item = String>) -> anyhow::Result<Self> {
        let mut input_path = None;
        let mut classification = None;
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--input" => {
                    input_path = Some(PathBuf::from(
                        arguments.next().context("--input requires a path")?,
                    ));
                }
                "--classification" => {
                    classification = Some(parse_classification(
                        &arguments
                            .next()
                            .context("--classification requires DOMESTIC or FDC")?,
                    )?);
                }
                _ => bail!("unknown argument {argument}"),
            }
        }
        Ok(Self {
            input_path: input_path.context("--input is required")?,
            classification: classification.context("--classification is required")?,
        })
    }
}

struct FetchArguments {
    config_path: PathBuf,
    output_dir: PathBuf,
    classifications: Vec<NmsNotamClassification>,
}

impl FetchArguments {
    fn parse(arguments: impl Iterator<Item = String>) -> anyhow::Result<Self> {
        let mut config_path = None;
        let mut output_dir = None;
        let mut classifications = Vec::new();
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--config" => {
                    config_path = Some(PathBuf::from(
                        arguments.next().context("--config requires a path")?,
                    ));
                }
                "--output" => {
                    output_dir = Some(PathBuf::from(
                        arguments.next().context("--output requires a path")?,
                    ));
                }
                "--classification" => {
                    let value = arguments
                        .next()
                        .context("--classification requires DOMESTIC or FDC")?;
                    classifications.push(parse_classification(&value)?);
                }
                _ => bail!("unknown argument {argument}"),
            }
        }

        if classifications.is_empty() {
            classifications = vec![
                NmsNotamClassification::Domestic,
                NmsNotamClassification::Fdc,
            ];
        }
        Ok(Self {
            config_path: config_path.context("--config is required")?,
            output_dir: output_dir.context("--output is required")?,
            classifications,
        })
    }
}

fn print_usage() {
    println!(
        "Usage:\n  nms-notams-fetch fetch --config PATH --output DIR [--classification DOMESTIC|FDC]...\n  nms-notams-fetch collect --config PATH --state-root DIR [--poll-seconds N] [--overlap-seconds N] [--duration-seconds N] [--max-polls N]\n  nms-notams-fetch inspect --input XML --classification DOMESTIC|FDC\n  nms-notams-fetch capture-fixture --initial-load DIR --state-root DIR --output DIR --captured-by-commit HASH"
    );
}

fn parse_positive_u64(label: &str, value: &str) -> anyhow::Result<u64> {
    let value = value
        .parse::<u64>()
        .with_context(|| format!("{label} must be an integer"))?;
    if value == 0 {
        bail!("{label} must be greater than zero");
    }
    Ok(value)
}

fn parse_classification(value: &str) -> anyhow::Result<NmsNotamClassification> {
    match value.trim().to_ascii_uppercase().as_str() {
        "DOMESTIC" | "DOM" => Ok(NmsNotamClassification::Domestic),
        "FDC" => Ok(NmsNotamClassification::Fdc),
        _ => bail!("unsupported NMS Initial Load classification {value}"),
    }
}
