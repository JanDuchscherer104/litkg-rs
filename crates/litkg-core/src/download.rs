use crate::config::RepoConfig;
use crate::model::{DownloadMode, PaperSourceRecord, ParseStatus};
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use tar::Archive;
use tempfile::tempdir;

#[derive(Debug, Clone, Copy)]
pub struct DownloadOptions {
    pub overwrite: bool,
    pub download_pdfs: bool,
}

pub fn download_registry_sources(
    config: &RepoConfig,
    registry: &[PaperSourceRecord],
    options: DownloadOptions,
) -> Result<Vec<PaperSourceRecord>> {
    fs::create_dir_all(&config.tex_root)?;
    if options.download_pdfs || config.download_pdfs {
        fs::create_dir_all(&config.pdf_root)?;
    }

    let mut updated = Vec::with_capacity(registry.len());
    for record in registry {
        let mut next = record.clone();
        match next.download_mode {
            DownloadMode::MetadataOnly => {}
            DownloadMode::ManifestSource | DownloadMode::ManifestSourcePlusPdf => {
                if let (Some(arxiv_id), Some(tex_dir)) =
                    (next.arxiv_id.clone(), next.tex_dir.clone())
                {
                    let source_url = format!("https://arxiv.org/e-print/{arxiv_id}");
                    let target_dir = config.tex_root.join(tex_dir);
                    if options.overwrite || !path_has_files(&target_dir) {
                        if target_dir.exists() {
                            fs::remove_dir_all(&target_dir)?;
                        }
                        fs::create_dir_all(&target_dir)?;
                        let temp_dir = tempdir()?;
                        let archive_path = temp_dir.path().join("source.bundle");
                        download_to_path(&source_url, &archive_path)?;
                        extract_archive(&archive_path, &target_dir)?;
                    }
                    next.has_local_tex = path_has_files(&target_dir);
                    if next.has_local_tex {
                        next.parse_status = ParseStatus::Downloaded;
                    }
                }

                if (options.download_pdfs || config.download_pdfs) && next.pdf_file.is_some() {
                    let arxiv_id = next.arxiv_id.clone().unwrap_or_default();
                    let pdf_path = config.pdf_root.join(next.pdf_file.clone().unwrap());
                    if options.overwrite || !pdf_path.is_file() {
                        let pdf_url = format!("https://arxiv.org/pdf/{arxiv_id}.pdf");
                        download_to_path(&pdf_url, &pdf_path)?;
                    }
                    next.has_local_pdf = pdf_path.is_file();
                }
            }
        }
        updated.push(next);
    }
    Ok(updated)
}

fn download_to_path(url: &str, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let status = Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg(url)
        .arg("-o")
        .arg(destination)
        .status()
        .with_context(|| format!("Failed to spawn curl for {url}"))?;
    if !status.success() {
        anyhow::bail!("curl failed for {url}");
    }
    Ok(())
}

fn extract_archive(bundle_path: &Path, target_dir: &Path) -> Result<()> {
    let bytes = fs::read(bundle_path)
        .with_context(|| format!("Failed to read {}", bundle_path.display()))?;
    if try_extract_tar_gz(&bytes, target_dir).is_ok() {
        return Ok(());
    }
    try_extract_plain_tar(&bytes, target_dir)
}

fn try_extract_tar_gz(bytes: &[u8], target_dir: &Path) -> Result<()> {
    let cursor = Cursor::new(bytes);
    let decoder = GzDecoder::new(cursor);
    let mut archive = Archive::new(decoder);
    extract_members(&mut archive, target_dir)
}

fn try_extract_plain_tar(bytes: &[u8], target_dir: &Path) -> Result<()> {
    let cursor = Cursor::new(bytes);
    let mut archive = Archive::new(cursor);
    extract_members(&mut archive, target_dir)
}

fn extract_members<R: Read>(archive: &mut Archive<R>, target_dir: &Path) -> Result<()> {
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let safe_path = normalize_archive_path(&entry.path()?.to_string_lossy())?;
        let destination = target_dir.join(safe_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(destination)?;
    }
    Ok(())
}

fn normalize_archive_path(raw: &str) -> Result<PathBuf> {
    let mut safe = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            _ => anyhow::bail!("Unsafe archive path {raw}"),
        }
    }
    if safe.as_os_str().is_empty() {
        anyhow::bail!("Empty archive path");
    }
    Ok(safe)
}

fn path_has_files(path: &Path) -> bool {
    path.is_dir()
        && path
            .read_dir()
            .map(|entries| {
                entries.flatten().any(|entry| {
                    let path = entry.path();
                    path.is_file() || path_has_files(&path)
                })
            })
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;
    use tar::{Builder, Header};

    fn tar_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = Builder::new(Vec::new());
        for (path, contents) in entries {
            let mut header = Header::new_gnu();
            header.set_path(path).unwrap();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, &contents[..]).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn write_bundle(path: &Path, gzipped: bool, entries: &[(&str, &[u8])]) {
        let bytes = tar_bytes(entries);
        if gzipped {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&bytes).unwrap();
            fs::write(path, encoder.finish().unwrap()).unwrap();
        } else {
            fs::write(path, bytes).unwrap();
        }
    }

    #[test]
    fn normalizes_safe_archive_paths() {
        assert_eq!(
            normalize_archive_path("paper/main.tex").unwrap(),
            PathBuf::from("paper/main.tex")
        );
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        assert!(normalize_archive_path("../paper/main.tex").is_err());
        assert!(normalize_archive_path("/tmp/main.tex").is_err());
        assert!(normalize_archive_path("").is_err());
    }

    #[test]
    fn extracts_tar_gz_archives() {
        let dir = tempdir().unwrap();
        let bundle = dir.path().join("source.bundle");
        let target = dir.path().join("out");
        write_bundle(&bundle, true, &[("paper/main.tex", b"\\section{Intro}")]);

        extract_archive(&bundle, &target).unwrap();

        assert_eq!(
            fs::read_to_string(target.join("paper/main.tex")).unwrap(),
            "\\section{Intro}"
        );
    }

    #[test]
    fn extracts_plain_tar_archives() {
        let dir = tempdir().unwrap();
        let bundle = dir.path().join("source.bundle");
        let target = dir.path().join("out");
        write_bundle(
            &bundle,
            false,
            &[("paper/appendix.tex", b"\\section{Appendix}")],
        );

        extract_archive(&bundle, &target).unwrap();

        assert_eq!(
            fs::read_to_string(target.join("paper/appendix.tex")).unwrap(),
            "\\section{Appendix}"
        );
    }
}
