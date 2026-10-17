use std::path::{Component, Path, PathBuf};

/// On-disk layout: `{root}/{user_id}/{session_id}/`
pub fn session_dir(root: &Path, user_id: &str, session_id: &str) -> PathBuf {
    root.join(user_id).join(session_id)
}

/// Public URL for a file inside a user-scoped session directory. Each segment is percent-encoded:
/// the file name is a tool's choice and the URL lands in an `href` (the route decodes it back).
pub fn public_url(user_id: &str, session_id: &str, path: &Path, root: &Path) -> Option<String> {
    let base = session_dir(root, user_id, session_id);
    let rel = path.strip_prefix(&base).ok()?;
    if rel.to_str()?.contains("..") {
        return None;
    }
    let mut segments = vec![urlencoding::encode(user_id), urlencoding::encode(session_id)];
    for component in rel.components() {
        let Component::Normal(name) = component else {
            return None;
        };
        segments.push(urlencoding::encode(name.to_str()?));
    }
    if segments.len() == 2 {
        return None;
    }
    Some(format!("/artifacts/{}", segments.join("/")))
}

/// Resolve a requested relative path safely inside `session_dir`.
pub fn resolve_file(session_root: &Path, relative: &str) -> Option<PathBuf> {
    let rel = Path::new(relative);
    if relative.is_empty() || relative.contains("..") {
        return None;
    }
    for component in rel.components() {
        if !matches!(component, Component::Normal(_)) {
            return None;
        }
    }
    let candidate = session_root.join(rel);
    let canonical_root = session_root.canonicalize().ok()?;
    let canonical_file = candidate.canonicalize().ok()?;
    if canonical_file.starts_with(&canonical_root) && canonical_file.is_file() {
        Some(canonical_file)
    } else {
        None
    }
}

pub fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Some("pdf") => "application/pdf",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_url_encodes_a_tool_chosen_file_name() {
        let root = Path::new("/tmp/agentrix-artifacts");
        let dir = session_dir(root, "user-1", "sess-1");
        let url = public_url("user-1", "sess-1", &dir.join("q3\" onmouseover=\"alert(1) <b>.pptx"), root);
        assert_eq!(url.as_deref(), Some("/artifacts/user-1/sess-1/q3%22%20onmouseover%3D%22alert%281%29%20%3Cb%3E.pptx"));
        let nested = public_url("user-1", "sess-1", &dir.join("out/Pitch deck.pptx"), root);
        assert_eq!(nested.as_deref(), Some("/artifacts/user-1/sess-1/out/Pitch%20deck.pptx"));
        assert_eq!(public_url("user-1", "sess-1", &dir, root), None);
        assert_eq!(public_url("user-1", "sess-1", &dir.join("a..b.pptx"), root), None);
    }
}
