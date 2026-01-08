use flate2::write::GzEncoder;
use ignore::WalkBuilder;
use std::path::Path;
use tar::Builder;

use crate::repo::challenges::metadata::CtfChallengeMetadata;

pub fn safe_pack_challenge(source_dir: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut gz_data = Vec::new();
    let mut tar_data = GzEncoder::new(&mut gz_data, flate2::Compression::default());

    {
        let mut archive = Builder::new(&mut tar_data);

        let walker = WalkBuilder::new(source_dir)
            .add_custom_ignore_filename(".plfignore")
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .ignore(false)
            .build();

        for entry in walker {
            let entry = entry?;
            let path = entry.path();

            if path == source_dir || path == source_dir.join("_plfanzen") {
                continue;
            }

            let relative_path = path.strip_prefix(source_dir)?;

            if path.file_name().and_then(|n| n.to_str()) == Some(".plfignore") {
                continue;
            }

            if path.is_file() {
                if path == source_dir.join("docker-compose.yml") {
                    let file_contents = std::fs::read_to_string(path)?;
                    let mut compose: compose_spec::Compose = serde_yaml::from_str(&file_contents)?;
                    let metadata = compose.extensions.get_mut("x-ctf-metadata");
                    if let Some(md) = metadata {
                        let mut metadata: CtfChallengeMetadata =
                            serde_yaml::from_value(md.clone())?;
                        metadata.flag = None;
                        metadata.flag_validation_fn = None;
                        *md = serde_yaml::to_value(metadata)?;
                    }
                    let new_compose_content = serde_yaml::to_string(&compose)?;
                    let mut header = tar::Header::new_gnu();
                    header.set_size(new_compose_content.as_bytes().len() as u64);
                    header.set_mode(0o644);
                    header.set_cksum();
                    header.set_mtime(
                        std::fs::metadata(path)?
                            .modified()?
                            .duration_since(std::time::UNIX_EPOCH)?
                            .as_secs(),
                    );
                    header.set_uid(1000);
                    header.set_gid(1000);
                    archive.append_data(
                        &mut header,
                        relative_path,
                        new_compose_content.as_bytes(),
                    )?;
                }
                archive.append_path_with_name(path, relative_path)?;
            } else if path.is_dir() {
                // This does not append the files inside the directory, just the directory itself
                archive.append_dir(relative_path, path)?;
            }
        }

        archive.finish()?;
    }

    tar_data.finish()?;
    Ok(gz_data)
}
