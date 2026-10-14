//! Minimal parser for RA2 component definition `.txt` files.
//!
//! These live inside `Components.cfz` (a CFL3 archive) and map a component class
//! to its mesh: `dir` + `model` resolve to `Components/<dir>/<model>`.

#[derive(Debug, Clone, Default)]
pub struct ComponentDef {
    pub name: String,
    pub dir: String,
    pub class: String,
    pub model: String,
    pub preview: String,
    pub standard: bool,
    /// All raw `key = value` pairs, lower-cased keys.
    pub fields: Vec<(String, String)>,
}

impl ComponentDef {
    /// Path of the mesh relative to the `Components` directory.
    pub fn mesh_rel_path(&self) -> String {
        if self.dir.is_empty() {
            self.model.clone()
        } else {
            format!("{}/{}", self.dir, self.model)
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Parse a `key = value` component definition (text).
pub fn parse(text: &str) -> ComponentDef {
    let mut d = ComponentDef::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
        match k.as_str() {
            "name" => d.name = v.clone(),
            "dir" => d.dir = v.clone(),
            "base" => d.class = v.clone(),
            "model" => d.model = v.clone(),
            "preview" => d.preview = v.clone(),
            "standard" => d.standard = v.trim() == "1",
            _ => {}
        }
        d.fields.push((k, v));
    }
    d
}
