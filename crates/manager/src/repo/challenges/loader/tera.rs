use std::path::Path;

use tera_with_js::TeraWithJs;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TeraError {
    #[error("Failed to read template file: {0}")]
    TemplateReadError(String),
    #[error("Template rendering error: {0:#?}")]
    TemplateRenderingError(#[from] tera_with_js::TeraWithJsError),
    #[error("JavaScript evaluation error: {0}")]
    JsEvalError(String),
}

pub fn load_tera_helpers(
    dir: &Path,
    eval_cb: &mut dyn FnMut(String) -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let ext = path
                .extension()
                .ok_or("No file extension")?
                .to_string_lossy();
            if ext == "js" {
                // This should be safe, but run it in a separate thread without any FS access to be sure it can't be doing anything malicious
                let code = std::fs::read_to_string(&path)?;
                eval_cb(code)?;
            }
        }
    }
    Ok(())
}

pub fn process_template(
    template_path: &std::path::Path,
    // The user or team requesting the challenge
    actor: &str,
    is_export: bool,
) -> Result<String, TeraError> {
    let tera = tera::Tera::default();

    let mut tera_ctx = tera::Context::new();
    tera_ctx.insert("actor", actor);
    tera_ctx.insert("is_export", &is_export);

    let mut tera_with_js = TeraWithJs::new(tera);

    let tera_dir = template_path.parent().unwrap().join("_plfanzen");

    if tera_dir.is_dir() {
        load_tera_helpers(&tera_dir, &mut |code| {
            tera_with_js.eval(code)?;
            Ok(())
        })
        .map_err(|e| TeraError::JsEvalError(e.to_string()))?;
    }

    let file_content = std::fs::read_to_string(template_path)
        .map_err(|e| TeraError::TemplateReadError(e.to_string()))?;
    Ok(tera_with_js.render_str(
        template_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string(),
        &file_content,
        &tera_ctx,
    )?)
}

pub fn render_dir_recursively(
    source_dir: &std::path::Path,
    dest_dir: &std::path::Path,
    actor: &str,
    is_export: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(source_dir)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dest_dir.join(entry.file_name());
        let absolute_path = std::fs::canonicalize(&path)?;
        if !absolute_path.starts_with(std::fs::canonicalize(source_dir)?) {
            return Err(format!(
                "Path traversal detected when rendering template: {}",
                path.to_string_lossy()
            )
            .into());
        }
        // Avoid infinite recursion
        if absolute_path == std::fs::canonicalize(source_dir)? {
            continue;
        }
        if path.is_dir() {
            std::fs::create_dir_all(&dest_path)?;
            render_dir_recursively(&path, &dest_path, actor, is_export)?;
        } else if path.is_file() {
            if path.extension().and_then(|s| s.to_str()) == Some("plftera") {
                let rendered_content = process_template(&path, actor, is_export)?;
                let dest_file_path = dest_path.with_extension("");
                std::fs::write(dest_file_path, rendered_content)?;
            } else {
                std::fs::copy(&path, &dest_path)?;
            }
        }
    }
    Ok(())
}
