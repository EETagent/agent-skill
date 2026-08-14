use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Result};

pub(crate) fn ensure_safe_relative_path(path: &Path, description: &str) -> Result<()> {
    let is_unsafe = path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });

    if is_unsafe {
        bail!("unsafe {description}: {}", path.display());
    }

    Ok(())
}

/// Resolves the deepest existing ancestor and then reattaches missing path components.
/// This preserves a not-yet-created destination while still resolving symlinked parents.
pub(crate) fn resolve_existing_ancestors(path: PathBuf) -> PathBuf {
    let mut current = path.as_path();
    let mut missing_components = Vec::new();

    loop {
        if let Ok(mut resolved) = current.canonicalize() {
            for component in missing_components.iter().rev() {
                resolved.push(component);
            }
            return resolved;
        }

        let Some(file_name) = current.file_name() else {
            return path;
        };
        missing_components.push(file_name.to_os_string());

        let Some(parent) = current.parent() else {
            return path;
        };
        current = parent;
    }
}
