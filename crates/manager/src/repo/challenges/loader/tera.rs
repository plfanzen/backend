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
) -> Result<String, TeraError> {
    let tera = tera::Tera::default();

    let mut tera_ctx = tera::Context::new();
    tera_ctx.insert("actor", actor);

    let mut tera_with_js = TeraWithJs::new(tera);

    let tera_dir = template_path.parent().unwrap().join("_tera");

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
