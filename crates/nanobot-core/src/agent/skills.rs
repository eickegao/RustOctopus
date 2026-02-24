//! Skills loader for agent capabilities.
//!
//! Skills are markdown files (`SKILL.md`) that teach the agent how to use
//! specific tools or perform certain tasks. They live in either the workspace
//! `skills/` directory or a built-in skills directory.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;


/// Information about a discovered skill.
#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub path: PathBuf,
    pub source: SkillSource,
}

/// Where a skill was loaded from.
#[derive(Debug, Clone, PartialEq)]
pub enum SkillSource {
    Workspace,
    Builtin,
}

/// Parsed frontmatter metadata from a SKILL.md file.
#[derive(Debug, Clone, Default)]
pub struct SkillMetadata {
    /// Raw key-value pairs from YAML frontmatter.
    pub fields: HashMap<String, String>,
}

impl SkillMetadata {
    /// Get a field value by key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(|s| s.as_str())
    }

    /// Get the skill description, falling back to empty string.
    pub fn description(&self) -> &str {
        self.fields.get("description").map(|s| s.as_str()).unwrap_or("")
    }

    /// Check if this skill is marked as always-loaded.
    pub fn is_always(&self) -> bool {
        self.fields
            .get("always")
            .map(|v| v == "true")
            .unwrap_or(false)
    }

    /// Parse nanobot-specific metadata from the `metadata` field (JSON).
    pub fn nanobot_metadata(&self) -> HashMap<String, serde_json::Value> {
        let raw = match self.fields.get("metadata") {
            Some(s) => s,
            None => return HashMap::new(),
        };
        match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::Object(map)) => {
                // Look for "nanobot" or "openclaw" key first
                if let Some(serde_json::Value::Object(inner)) = map.get("nanobot").or_else(|| map.get("openclaw")) {
                    inner.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                } else {
                    map.into_iter().collect()
                }
            }
            _ => HashMap::new(),
        }
    }
}

/// Loader for agent skills from workspace and built-in directories.
pub struct SkillsLoader {
    workspace: PathBuf,
    workspace_skills: PathBuf,
    builtin_skills: Option<PathBuf>,
}

impl SkillsLoader {
    /// Create a new SkillsLoader for the given workspace.
    ///
    /// Built-in skills directory is optional. If not provided, only workspace
    /// skills will be discovered.
    pub fn new(workspace: PathBuf, builtin_skills: Option<PathBuf>) -> Self {
        let workspace_skills = workspace.join("skills");
        Self {
            workspace,
            workspace_skills,
            builtin_skills,
        }
    }

    /// List all available skills from workspace and built-in directories.
    ///
    /// Workspace skills take priority over built-in skills with the same name.
    /// When `filter_unavailable` is true, skills with unmet requirements are
    /// excluded (currently always returns true for availability).
    pub fn list_skills(&self, filter_unavailable: bool) -> Vec<SkillInfo> {
        let mut skills = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        // Workspace skills (highest priority)
        if self.workspace_skills.is_dir() {
            if let Ok(entries) = fs::read_dir(&self.workspace_skills) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let skill_file = path.join("SKILL.md");
                        if skill_file.exists() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            seen_names.insert(name.clone());
                            skills.push(SkillInfo {
                                name,
                                path: skill_file,
                                source: SkillSource::Workspace,
                            });
                        }
                    }
                }
            }
        }

        // Built-in skills
        if let Some(builtin_dir) = &self.builtin_skills {
            if builtin_dir.is_dir() {
                if let Ok(entries) = fs::read_dir(builtin_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let skill_file = path.join("SKILL.md");
                            let name = entry.file_name().to_string_lossy().to_string();
                            if skill_file.exists() && !seen_names.contains(&name) {
                                seen_names.insert(name.clone());
                                skills.push(SkillInfo {
                                    name,
                                    path: skill_file,
                                    source: SkillSource::Builtin,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Filter by requirements (currently a no-op: always available)
        if filter_unavailable {
            skills.retain(|s| self.check_requirements(s));
        }

        skills
    }

    /// Load the raw content of a skill's SKILL.md by name.
    ///
    /// Checks workspace first, then built-in directory.
    pub fn load_skill(&self, name: &str) -> Option<String> {
        // Check workspace first
        let workspace_skill = self.workspace_skills.join(name).join("SKILL.md");
        if workspace_skill.exists() {
            return fs::read_to_string(&workspace_skill).ok();
        }

        // Check built-in
        if let Some(builtin_dir) = &self.builtin_skills {
            let builtin_skill = builtin_dir.join(name).join("SKILL.md");
            if builtin_skill.exists() {
                return fs::read_to_string(&builtin_skill).ok();
            }
        }

        None
    }

    /// Load specific skills and format them for inclusion in agent context.
    ///
    /// Skills are stripped of YAML frontmatter and formatted with headers.
    /// Multiple skills are separated by `"\n\n---\n\n"`.
    pub fn load_skills_for_context(&self, skill_names: &[String]) -> String {
        let mut parts = Vec::new();
        for name in skill_names {
            if let Some(content) = self.load_skill(name) {
                let stripped = strip_frontmatter(&content);
                parts.push(format!("### Skill: {}\n\n{}", name, stripped));
            }
        }
        parts.join("\n\n---\n\n")
    }

    /// Build an XML summary of all skills for progressive loading.
    ///
    /// The agent can read the full skill content using `read_file` when needed.
    pub fn build_skills_summary(&self) -> String {
        let all_skills = self.list_skills(false);
        if all_skills.is_empty() {
            return String::new();
        }

        let mut lines = vec!["<skills>".to_string()];
        for s in &all_skills {
            let name = escape_xml(&s.name);
            let path = s.path.display();
            let desc = escape_xml(&self.get_skill_description(&s.name));
            let available = self.check_requirements(s);

            lines.push(format!("  <skill available=\"{}\">", available));
            lines.push(format!("    <name>{}</name>", name));
            lines.push(format!("    <description>{}</description>", desc));
            lines.push(format!("    <location>{}</location>", path));
            lines.push("  </skill>".to_string());
        }
        lines.push("</skills>".to_string());

        lines.join("\n")
    }

    /// Get skills marked as `always=true` that meet requirements.
    pub fn get_always_skills(&self) -> Vec<String> {
        let mut result = Vec::new();
        for s in self.list_skills(true) {
            if let Some(meta) = self.get_skill_metadata(&s.name) {
                if meta.is_always() {
                    result.push(s.name.clone());
                }
                // Also check nanobot metadata
                let nb_meta = meta.nanobot_metadata();
                if nb_meta.get("always") == Some(&serde_json::Value::Bool(true))
                    && !result.contains(&s.name)
                {
                    result.push(s.name.clone());
                }
            }
        }
        result
    }

    /// Parse YAML frontmatter from a skill's SKILL.md file.
    ///
    /// Returns `None` if the skill has no frontmatter or does not exist.
    pub fn get_skill_metadata(&self, name: &str) -> Option<SkillMetadata> {
        let content = self.load_skill(name)?;

        if !content.starts_with("---") {
            return None;
        }

        let re = Regex::new(r"(?s)^---\n(.*?)\n---").ok()?;
        let captures = re.captures(&content)?;
        let frontmatter = captures.get(1)?.as_str();

        let mut fields = HashMap::new();
        for line in frontmatter.lines() {
            if let Some(pos) = line.find(':') {
                let key = line[..pos].trim().to_string();
                let value = line[pos + 1..].trim().trim_matches(|c| c == '"' || c == '\'').to_string();
                fields.insert(key, value);
            }
        }

        Some(SkillMetadata { fields })
    }

    /// Get the description for a skill. Falls back to the skill name.
    fn get_skill_description(&self, name: &str) -> String {
        if let Some(meta) = self.get_skill_metadata(name) {
            let desc = meta.description();
            if !desc.is_empty() {
                return desc.to_string();
            }
        }
        name.to_string()
    }

    /// Check if a skill's requirements are met.
    ///
    /// Currently always returns `true` (requirement checking not yet implemented).
    fn check_requirements(&self, _skill: &SkillInfo) -> bool {
        // Simplified: always return true for now
        true
    }

    /// Get the workspace path.
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// Get the workspace skills directory path.
    pub fn workspace_skills_dir(&self) -> &Path {
        &self.workspace_skills
    }
}

/// Remove YAML frontmatter from markdown content.
pub fn strip_frontmatter(content: &str) -> &str {
    if !content.starts_with("---") {
        return content;
    }
    if let Ok(re) = Regex::new(r"(?s)^---\n.*?\n---\n") {
        if let Some(m) = re.find(content) {
            return content[m.end()..].trim();
        }
    }
    content
}

/// Escape XML special characters.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_skill(dir: &Path, name: &str, content: &str) {
        let skill_dir = dir.join("skills").join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn test_list_skills_empty() {
        let dir = TempDir::new().unwrap();
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        assert!(loader.list_skills(true).is_empty());
    }

    #[test]
    fn test_list_skills_workspace() {
        let dir = TempDir::new().unwrap();
        create_skill(dir.path(), "coding", "# Coding Skill\nWrite code.");
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        let skills = loader.list_skills(true);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "coding");
        assert_eq!(skills[0].source, SkillSource::Workspace);
    }

    #[test]
    fn test_list_skills_builtin() {
        let dir = TempDir::new().unwrap();
        let builtin_dir = TempDir::new().unwrap();
        let skill_dir = builtin_dir.path().join("web-search");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Web Search").unwrap();

        let loader = SkillsLoader::new(
            dir.path().to_path_buf(),
            Some(builtin_dir.path().to_path_buf()),
        );
        let skills = loader.list_skills(true);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "web-search");
        assert_eq!(skills[0].source, SkillSource::Builtin);
    }

    #[test]
    fn test_workspace_overrides_builtin() {
        let dir = TempDir::new().unwrap();
        let builtin_dir = TempDir::new().unwrap();

        // Create same skill in both
        create_skill(dir.path(), "coding", "# Workspace Coding");
        let builtin_skill = builtin_dir.path().join("coding");
        fs::create_dir_all(&builtin_skill).unwrap();
        fs::write(builtin_skill.join("SKILL.md"), "# Builtin Coding").unwrap();

        let loader = SkillsLoader::new(
            dir.path().to_path_buf(),
            Some(builtin_dir.path().to_path_buf()),
        );
        let skills = loader.list_skills(true);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].source, SkillSource::Workspace);
    }

    #[test]
    fn test_load_skill() {
        let dir = TempDir::new().unwrap();
        create_skill(dir.path(), "coding", "# Coding\nWrite great code.");
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        let content = loader.load_skill("coding").unwrap();
        assert!(content.contains("Write great code"));
    }

    #[test]
    fn test_load_skill_not_found() {
        let dir = TempDir::new().unwrap();
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        assert!(loader.load_skill("nonexistent").is_none());
    }

    #[test]
    fn test_load_skills_for_context() {
        let dir = TempDir::new().unwrap();
        create_skill(dir.path(), "coding", "# Coding\nWrite code.");
        create_skill(dir.path(), "testing", "# Testing\nWrite tests.");
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        let context = loader.load_skills_for_context(&[
            "coding".to_string(),
            "testing".to_string(),
        ]);
        assert!(context.contains("### Skill: coding"));
        assert!(context.contains("### Skill: testing"));
        assert!(context.contains("---"));
    }

    #[test]
    fn test_strip_frontmatter() {
        let content = "---\ntitle: Test\ndescription: A test skill\n---\n# Actual Content\nBody here.";
        let stripped = strip_frontmatter(content);
        assert_eq!(stripped, "# Actual Content\nBody here.");
    }

    #[test]
    fn test_strip_frontmatter_no_frontmatter() {
        let content = "# Just Content\nNo frontmatter.";
        let stripped = strip_frontmatter(content);
        assert_eq!(stripped, content);
    }

    #[test]
    fn test_get_skill_metadata() {
        let dir = TempDir::new().unwrap();
        create_skill(
            dir.path(),
            "coding",
            "---\ntitle: Coding\ndescription: Write code\nalways: true\n---\n# Coding",
        );
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        let meta = loader.get_skill_metadata("coding").unwrap();
        assert_eq!(meta.get("title"), Some("Coding"));
        assert_eq!(meta.get("description"), Some("Write code"));
        assert!(meta.is_always());
    }

    #[test]
    fn test_get_skill_metadata_no_frontmatter() {
        let dir = TempDir::new().unwrap();
        create_skill(dir.path(), "basic", "# Basic Skill\nNo frontmatter.");
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        assert!(loader.get_skill_metadata("basic").is_none());
    }

    #[test]
    fn test_get_always_skills() {
        let dir = TempDir::new().unwrap();
        create_skill(
            dir.path(),
            "always-on",
            "---\nalways: true\ndescription: Always loaded\n---\n# Always",
        );
        create_skill(
            dir.path(),
            "on-demand",
            "---\nalways: false\ndescription: On demand\n---\n# On Demand",
        );
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        let always = loader.get_always_skills();
        assert_eq!(always.len(), 1);
        assert_eq!(always[0], "always-on");
    }

    #[test]
    fn test_build_skills_summary() {
        let dir = TempDir::new().unwrap();
        create_skill(
            dir.path(),
            "coding",
            "---\ndescription: Write code\n---\n# Coding",
        );
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        let summary = loader.build_skills_summary();
        assert!(summary.contains("<skills>"));
        assert!(summary.contains("</skills>"));
        assert!(summary.contains("<name>coding</name>"));
        assert!(summary.contains("<description>Write code</description>"));
        assert!(summary.contains("available=\"true\""));
    }

    #[test]
    fn test_build_skills_summary_empty() {
        let dir = TempDir::new().unwrap();
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        assert!(loader.build_skills_summary().is_empty());
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("a < b & c > d"), "a &lt; b &amp; c &gt; d");
    }

    #[test]
    fn test_load_skills_for_context_strips_frontmatter() {
        let dir = TempDir::new().unwrap();
        create_skill(
            dir.path(),
            "coding",
            "---\ntitle: Coding\n---\n# Coding\nBody content.",
        );
        let loader = SkillsLoader::new(dir.path().to_path_buf(), None);
        let context = loader.load_skills_for_context(&["coding".to_string()]);
        assert!(!context.contains("---\ntitle:"));
        assert!(context.contains("Body content."));
    }
}
