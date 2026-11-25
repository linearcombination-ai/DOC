//! Manifest reading (`book_codes_and_names_from_manifest`): `manifest.yaml` (a `projects` list)
//! or `manifest.json` (one `project`), looked for one directory down, then at the root.

use doc_model::tree::Tree;
use serde_json::Value as Json;
use serde_yaml_ng::Value as Yaml;

use crate::books::Failure;

#[derive(Clone, Debug, PartialEq)]
pub enum Title {
    Text(String),
    /// A scalar that is not text (number, bool, null, list, map): DOC calls `.strip()` on it.
    Other(String),
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub path: String,
    pub yaml: bool,
}

/// DOC's glob cascade; the first (sorted) match of the first pattern that matches anything.
pub fn candidate(tree: &Tree, dir: &str) -> Option<Candidate> {
    for (ext, yaml) in [("yaml", true), ("json", false)] {
        let deep = tree.glob(&format!("{dir}/**/manifest.{ext}"));
        let found = if deep.is_empty() { tree.glob(&format!("{dir}/manifest.{ext}")) } else { deep };
        if let Some(path) = found.into_iter().next() {
            return Some(Candidate { path, yaml });
        }
    }
    None
}

fn yaml_title(v: &Yaml) -> Title {
    match v {
        Yaml::String(s) => Title::Text(s.clone()),
        Yaml::Null => Title::Other("null".into()),
        Yaml::Bool(_) => Title::Other("a boolean".into()),
        Yaml::Number(_) => Title::Other("a number".into()),
        _ => Title::Other("a list or map".into()),
    }
}

fn json_title(v: &Json) -> Title {
    match v {
        Json::String(s) => Title::Text(s.clone()),
        Json::Null => Title::Other("null".into()),
        Json::Bool(_) => Title::Other("a boolean".into()),
        Json::Number(_) => Title::Other("a number".into()),
        _ => Title::Other("a list or map".into()),
    }
}

/// Code to title, in manifest order (a later duplicate wins, as in a Python dict comprehension).
pub fn book_names(tree: &Tree, dir: &str) -> Result<Vec<(String, Title)>, Failure> {
    let Some(c) = candidate(tree, dir) else { return Ok(Vec::new()) };
    let text = match tree.read(&c.path) {
        Ok(Some(t)) => t,
        Ok(None) => "",
        Err(e) => return Err(fail("UnreadableFile", e, dir)),
    };
    let mut out: Vec<(String, Title)> = Vec::new();
    let mut put = |code: String, title: Title| match out.iter_mut().find(|(k, _)| *k == code) {
        Some(slot) => slot.1 = title,
        None => out.push((code, title)),
    };
    if c.yaml {
        // PyYAML's scanner skips a BOM at the start of the stream; serde_yaml_ng rejects it
        // (found by the oracle comparison: fr_ulb and tl_ulb manifests start with U+FEFF).
        let doc: Yaml = serde_yaml_ng::from_str(text.strip_prefix('\u{feff}').unwrap_or(text)).map_err(|e| fail("ManifestParse", format!("{}: {e}", c.path), dir))?;
        let projects = doc
            .get("projects")
            .ok_or_else(|| fail("ManifestShape", format!("{}: no `projects` (DOC raises KeyError/TypeError)", c.path), dir))?;
        let list = projects.as_sequence().ok_or_else(|| fail("ManifestShape", format!("{}: `projects` is not a list", c.path), dir))?;
        for book in list {
            let identifier = book.get("identifier").ok_or_else(|| fail("ManifestShape", format!("{}: a project has no `identifier`", c.path), dir))?;
            let title = book.get("title").ok_or_else(|| fail("ManifestShape", format!("{}: a project has no `title`", c.path), dir))?;
            if let Yaml::String(code) = identifier {
                put(code.clone(), yaml_title(title));
            }
        }
    } else {
        let doc: Json = serde_json::from_str(text).map_err(|e| fail("ManifestParse", format!("{}: {e}", c.path), dir))?;
        let project = doc.get("project").ok_or_else(|| fail("ManifestShape", format!("{}: no `project`", c.path), dir))?;
        let id = project.get("id").ok_or_else(|| fail("ManifestShape", format!("{}: project has no `id`", c.path), dir))?;
        let name = project.get("name").ok_or_else(|| fail("ManifestShape", format!("{}: project has no `name`", c.path), dir))?;
        if let Json::String(code) = id {
            put(code.clone(), json_title(name));
        }
    }
    Ok(out)
}

fn fail(kind: &'static str, detail: String, repo: &str) -> Failure {
    Failure::manifest(kind, detail, repo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_manifest_with_bom_is_read() {
        let mut tree = Tree::default();
        let data = doc_model::tree::TreeData {
            files: vec!["r/manifest.yaml".into()],
            texts: [("r/manifest.yaml".to_string(), Some("\u{feff}---\nprojects:\n- identifier: gen\n  title: Genèse\n".to_string()))].into_iter().collect(),
        };
        tree.add_dir("", &data);
        let names = book_names(&tree, "r").expect("a BOM must not break the manifest");
        assert_eq!(names, vec![("gen".to_string(), Title::Text("Genèse".to_string()))]);
    }
}
