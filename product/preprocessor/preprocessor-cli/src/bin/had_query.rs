use anyhow::{bail, Context};
use preprocessor_core::nav_kv::NavKvRoot;
use std::{
    env, fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use zip::ZipArchive;

fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        bail!("usage: had-query <had-dir-or-zip> <key>");
    }
    let source = PathBuf::from(args.remove(0));
    let key = args.remove(0);
    let value = if source.is_dir() {
        query_had_dir(&source, &key)?
    } else {
        query_had_zip(&source, &key)?
    };
    let value = value.with_context(|| format!("missing HAD key {key}"))?;
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&value) {
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else if let Ok(text) = std::str::from_utf8(&value) {
        println!("{text}");
    } else {
        bail!("value for {key} is not UTF-8 or JSON");
    }
    Ok(())
}

fn query_had_dir(dir: &Path, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let root_bytes = fs::read(dir.join("root"))
        .with_context(|| format!("failed to read {}", dir.join("root").display()))?;
    let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
    Ok(root.extract_value(key, |page_index| {
        fs::read(dir.join(format!("values_{page_index:04}"))).ok()
    }))
}

fn query_had_zip(path: &Path, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("failed to read zip {}", path.display()))?;
    let root_bytes = read_zip_member(&mut archive, "root")?;
    let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
    Ok(root.extract_value(key, |page_index| {
        read_zip_member(&mut archive, &format!("values_{page_index:04}")).ok()
    }))
}

fn read_zip_member(archive: &mut ZipArchive<File>, name: &str) -> anyhow::Result<Vec<u8>> {
    let mut file = archive
        .by_name(name)
        .with_context(|| format!("missing zip member {name}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("failed to read zip member {name}"))?;
    Ok(bytes)
}
