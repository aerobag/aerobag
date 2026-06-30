use preprocessor_core::CaptureEntry;

mod diagnostics;
mod labels;
mod png;
mod tool_invocation;

pub use diagnostics::command_output_diagnostic_summary;
pub use labels::sanitize_label;
pub use png::{append_pngs_vertical, flatten_png_onto_white, write_thumbnail_from_png};
pub use tool_invocation::{
    run_tool_runner, ToolInvocation, ToolLogPaths, ToolOutcome, TOOL_RUNNER_ARG,
    TOOL_RUNNER_EXE_ENV,
};

pub fn comparison_targets(entry: &CaptureEntry) -> Vec<&'static str> {
    let mut targets = vec!["zip_members", "package_hashes"];
    if entry.tile_paths.is_some() {
        targets.push("tile_paths");
    }
    if entry.source_urls.is_some() {
        targets.push("source_urls");
    }
    targets
}
