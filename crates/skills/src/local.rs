//! A versioned, user-authored skill library on disk.
//!
//! Layout (the same one the UI documents):
//!
//! ```text
//! <root>/index.json                      versions, hashes, installed copies
//! <root>/library/<id>/<version>/SKILL.md immutable version snapshots
//! <root>/installed/<id>/SKILL.md         the active copy an agent reads
//! <root>/backups/<id>/<timestamp>/       preserved modified copies
//! ```
//!
//! A skill is data. Registration and installation copy files; they never
//! execute a script, follow a symlink out of the library, or let a path
//! traverse above its skill directory. `SKILL.md` may carry `scripts/`,
//! `references/`, and `assets/` as ordinary files.
//!
//! Versions are immutable. Revising publishes a new snapshot; installing
//! chooses one. That separation is what makes rollback a copy rather than a
//! reconstruction, and it is what protects a user's local edits from being
//! silently replaced by the next revision.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// One supporting file supplied alongside `SKILL.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFileInput {
    /// Relative path inside the skill folder (`references/style.md`).
    pub path: String,
    /// File contents, treated as data.
    pub content: String,
}

/// Input for importing a document from the live catalog.
#[derive(Debug, Clone)]
pub struct ImportSkillInput {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub content: String,
    /// Where the document came from (`voltagent`, a URL, …), kept for provenance.
    pub source: String,
    /// Publish a new version when the id already exists.
    pub update: bool,
}

/// One immutable version shown in the version list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersion {
    pub version: String,
    pub hash: String,
    pub created_at: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

/// A skill row in the list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub example_prompt: Option<String>,
    pub latest_version: String,
    pub versions: Vec<SkillVersion>,
    #[serde(default)]
    pub installed_version: Option<String>,
    pub installed_modified: bool,
    /// Where the skill came from (`local`, `voltagent`, `learned:run:<id>`).
    #[serde(default)]
    pub provenance: Option<String>,
}

/// A full skill document plus its metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillView {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub example_prompt: Option<String>,
    pub version: String,
    pub hash: String,
    /// `installed` when the view came from the active copy, `library` otherwise.
    pub source: String,
    #[serde(default)]
    pub installed_version: Option<String>,
    pub installed_modified: bool,
    /// Where the skill came from (`local`, `voltagent`, `learned:run:<id>`).
    #[serde(default)]
    pub provenance: Option<String>,
    pub content: String,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    skills: BTreeMap<String, Record>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Record {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    example_prompt: Option<String>,
    #[serde(default)]
    source: Option<String>,
    /// Newest first.
    #[serde(default)]
    versions: Vec<SkillVersion>,
    #[serde(default)]
    installed: Option<Installed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Installed {
    version: String,
    hash: String,
    installed_at: String,
    #[serde(default)]
    files: Vec<String>,
}

/// The on-disk library. `Default` resolves `~/RAVENBOT/skills`, with
/// `RAVENBOT_SKILLS_DIR` as the test/relocation override.
#[derive(Debug, Clone)]
pub struct LocalSkillLibrary {
    root: PathBuf,
}

impl Default for LocalSkillLibrary {
    fn default() -> Self {
        let root = std::env::var("RAVENBOT_SKILLS_DIR")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| ravenbot_core::expand_home(&value))
            .unwrap_or_else(|| ravenbot_core::raven_root().join("skills"));
        Self { root }
    }
}

impl LocalSkillLibrary {
    /// Open (or create) a library at an explicit root. Used by tests and by
    /// callers that keep skills next to a different workspace.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every skill, newest library version first in each row.
    pub fn list(&self) -> Result<Vec<SkillSummary>, String> {
        let index = self.read_index()?;
        let mut out = Vec::with_capacity(index.skills.len());
        for (id, record) in &index.skills {
            let installed = record.installed.as_ref();
            out.push(SkillSummary {
                id: id.clone(),
                name: display_name(record, id),
                description: record.description.clone(),
                example_prompt: record.example_prompt.clone(),
                latest_version: record
                    .versions
                    .first()
                    .map(|version| version.version.clone())
                    .unwrap_or_default(),
                versions: record.versions.clone(),
                installed_version: installed.map(|value| value.version.clone()),
                installed_modified: self.installed_modified(id, record),
                provenance: record.source.clone(),
            });
        }
        Ok(out)
    }

    /// Read one version. With no version, prefer the installed copy.
    pub fn read(&self, id: &str, version: Option<&str>) -> Result<SkillView, String> {
        validate_id(id)?;
        let index = self.read_index()?;
        let record = index
            .skills
            .get(id)
            .ok_or_else(|| format!("Unknown local skill '{id}'."))?;

        let (source, selected) = match version {
            Some(version) => ("library", version.to_string()),
            None => match &record.installed {
                Some(installed) => ("installed", installed.version.clone()),
                None => (
                    "library",
                    record
                        .versions
                        .first()
                        .map(|value| value.version.clone())
                        .ok_or_else(|| format!("Local skill '{id}' has no versions."))?,
                ),
            },
        };

        let dir = if source == "installed" {
            self.installed_dir(id)
        } else {
            self.version_dir(id, &selected)
        };
        if !dir.is_dir() {
            return Err(format!(
                "The {} copy of '{id}' at version {selected} is missing from disk.",
                if source == "installed" {
                    "installed"
                } else {
                    "library"
                }
            ));
        }

        let content = std::fs::read_to_string(dir.join("SKILL.md")).map_err(|error| {
            format!(
                "Could not read '{}': {error}",
                dir.join("SKILL.md").display()
            )
        })?;
        let files = self.list_files(&dir);
        let hash = dir_hash(&dir)?;

        Ok(SkillView {
            id: id.to_string(),
            name: display_name(record, id),
            description: record.description.clone(),
            example_prompt: record.example_prompt.clone(),
            version: selected,
            hash,
            source: source.to_string(),
            installed_version: record.installed.as_ref().map(|value| value.version.clone()),
            installed_modified: self.installed_modified(id, record),
            provenance: record.source.clone(),
            content,
            files,
        })
    }

    /// Publish the first version of a new skill. Creation does not install it.
    pub fn create(
        &self,
        id: &str,
        content: &str,
        files: &[SkillFileInput],
        note: Option<String>,
    ) -> Result<SkillView, String> {
        self.create_with_source(id, content, files, note, "local")
    }

    /// Publish the first version of a new skill with an explicit provenance
    /// string, such as `learned:run:<id>`. The source is stored in the index
    /// and surfaced by [`SkillSummary::provenance`] and [`SkillView::provenance`].
    ///
    /// The same library, validation and file-writing path as [`Self::create`]
    /// is used, so a learned skill cannot end up outside the library or with a
    /// shape the agent tools would refuse.
    pub fn create_with_source(
        &self,
        id: &str,
        content: &str,
        files: &[SkillFileInput],
        note: Option<String>,
        source: &str,
    ) -> Result<SkillView, String> {
        validate_id(id)?;
        validate_content(content)?;
        validate_files(files)?;
        let mut index = self.read_index()?;
        if index.skills.contains_key(id) {
            return Err(format!(
                "A local skill with id '{id}' already exists. Use revise_skill to add a version."
            ));
        }

        let (meta, _) = parse_document(content);
        let version = "0.1.0".to_string();
        let hash = self.write_version(id, &version, content, files)?;
        let source = source.trim();
        let record = Record {
            name: meta.name.unwrap_or_else(|| id.to_string()),
            description: meta.description.unwrap_or_default(),
            example_prompt: meta.example_prompt,
            source: Some(if source.is_empty() {
                "local".to_string()
            } else {
                source.to_string()
            }),
            versions: vec![SkillVersion {
                version: version.clone(),
                hash,
                created_at: now(),
                note,
                files: files.iter().map(|file| file.path.clone()).collect(),
            }],
            installed: None,
        };
        index.skills.insert(id.to_string(), record);
        self.write_index(&index)?;
        self.read(id, Some(&version))
    }

    /// Publish a new patch version. `base_version` must be the latest, so a
    /// concurrent revision cannot silently overwrite a newer one.
    pub fn revise(
        &self,
        id: &str,
        base_version: &str,
        content: &str,
        files: &[SkillFileInput],
        note: Option<String>,
    ) -> Result<SkillView, String> {
        validate_id(id)?;
        validate_content(content)?;
        validate_files(files)?;
        let mut index = self.read_index()?;
        let record = index
            .skills
            .get_mut(id)
            .ok_or_else(|| format!("Unknown local skill '{id}'."))?;

        let latest = record
            .versions
            .first()
            .map(|value| value.version.clone())
            .ok_or_else(|| format!("Local skill '{id}' has no versions."))?;
        if latest != base_version {
            return Err(format!(
                "Revision refused: '{id}' is at {latest}, but this edit was based on {base_version}. Read the current version before revising."
            ));
        }

        let next = bump_patch(&latest);
        let hash = self.write_version(id, &next, content, files)?;
        let (meta, _) = parse_document(content);
        record.versions.insert(
            0,
            SkillVersion {
                version: next.clone(),
                hash,
                created_at: now(),
                note,
                files: files.iter().map(|file| file.path.clone()).collect(),
            },
        );
        if let Some(name) = meta.name {
            record.name = name;
        }
        if let Some(description) = meta.description {
            record.description = description;
        }
        if meta.example_prompt.is_some() {
            record.example_prompt = meta.example_prompt;
        }
        self.write_index(&index)?;
        self.read(id, Some(&next))
    }

    /// Install one exact version. A modified installed copy is refused unless
    /// `force`, in which case the current copy is preserved under `backups/`.
    pub fn install(&self, id: &str, version: &str, force: bool) -> Result<SkillView, String> {
        validate_id(id)?;
        validate_version(version)?;
        let mut index = self.read_index()?;
        let record = index
            .skills
            .get_mut(id)
            .ok_or_else(|| format!("Unknown local skill '{id}'."))?;
        let target = record
            .versions
            .iter()
            .find(|entry| entry.version == version)
            .ok_or_else(|| format!("Local skill '{id}' has no version {version}."))?
            .clone();

        let modified = self.installed_modified(id, record);
        if modified && !force {
            return Err(format!(
                "The installed copy of '{id}' has local modifications. Replacing it would lose them. Use force=true to back up the edits and install {version}."
            ));
        }

        let source = self.version_dir(id, version);
        if !source.is_dir() {
            return Err(format!(
                "The library copy of '{id}' at {version} is missing from disk."
            ));
        }

        if modified {
            self.backup_installed(id)?;
        }

        let destination = self.installed_dir(id);
        if destination.exists() {
            remove_dir(&destination)?;
        }
        copy_dir(&source, &destination)?;
        let hash = dir_hash(&destination)?;

        record.installed = Some(Installed {
            version: version.to_string(),
            hash,
            installed_at: now(),
            files: target.files.clone(),
        });
        self.write_index(&index)?;
        self.read(id, None)
    }

    /// Import a catalog document. A collision needs `update: true`; an update
    /// with identical content is a no-op rather than a new empty version.
    pub fn import_document(&self, input: ImportSkillInput) -> Result<SkillView, String> {
        validate_id(&input.id)?;
        validate_content(&input.content)?;
        let mut index = self.read_index()?;
        let (meta, _) = parse_document(&input.content);
        let name = input.name.or(meta.name);
        let description = input.description.or(meta.description);
        let example_prompt = meta.example_prompt;

        if !index.skills.contains_key(&input.id) {
            let version = "1.0.0".to_string();
            let hash = self.write_version(&input.id, &version, &input.content, &[])?;
            let record = Record {
                name: name.unwrap_or_else(|| input.id.clone()),
                description: description.clone().unwrap_or_default(),
                example_prompt,
                source: Some(input.source.clone()),
                versions: vec![SkillVersion {
                    version: version.clone(),
                    hash: hash.clone(),
                    created_at: now(),
                    note: Some(format!("Imported from {}", input.source)),
                    files: Vec::new(),
                }],
                installed: Some(Installed {
                    version: version.clone(),
                    hash,
                    installed_at: now(),
                    files: Vec::new(),
                }),
            };
            self.install_copy_from_library(&input.id, &version)?;
            index.skills.insert(input.id.clone(), record);
            self.write_index(&index)?;
            return self.read(&input.id, None);
        }

        if !input.update {
            return Err(format!(
                "A local skill with id '{}' already exists. Pass update=true to publish the catalog version as a new revision.",
                input.id
            ));
        }

        let record = index.skills.get_mut(&input.id).expect("checked above");
        let latest = record
            .versions
            .first()
            .cloned()
            .ok_or_else(|| format!("Local skill '{}' has no versions.", input.id))?;
        let incoming_hash = content_hash(&input.content);
        if content_hash(
            &std::fs::read_to_string(
                self.version_dir(&input.id, &latest.version)
                    .join("SKILL.md"),
            )
            .unwrap_or_default(),
        ) == incoming_hash
        {
            return self.read(&input.id, None);
        }
        if self.installed_modified(&input.id, record) {
            return Err(format!(
                "The installed copy of '{}' has local modifications. Install a version from the library with force=true first, or revert the edits.",
                input.id
            ));
        }

        let next = bump_patch(&latest.version);
        let hash = self.write_version(&input.id, &next, &input.content, &[])?;
        record.versions.insert(
            0,
            SkillVersion {
                version: next.clone(),
                hash: hash.clone(),
                created_at: now(),
                note: Some(format!("Imported from {}", input.source)),
                files: Vec::new(),
            },
        );
        if let Some(name) = name {
            record.name = name;
        }
        if let Some(description) = description {
            record.description = description;
        }
        if example_prompt.is_some() {
            record.example_prompt = example_prompt;
        }
        record.installed = Some(Installed {
            version: next.clone(),
            hash,
            installed_at: now(),
            files: Vec::new(),
        });
        self.install_copy_from_library(&input.id, &next)?;
        self.write_index(&index)?;
        self.read(&input.id, None)
    }

    /// The installed copy differs from the hash recorded at install time.
    fn installed_modified(&self, id: &str, record: &Record) -> bool {
        let Some(installed) = &record.installed else {
            return false;
        };
        let dir = self.installed_dir(id);
        if !dir.is_dir() {
            // A recorded install whose folder vanished is a broken state worth
            // surfacing as "modified" rather than as an unexplained reinstall.
            return true;
        }
        match dir_hash(&dir) {
            Ok(hash) => hash != installed.hash,
            Err(_) => true,
        }
    }

    fn backup_installed(&self, id: &str) -> Result<(), String> {
        let source = self.installed_dir(id);
        if !source.is_dir() {
            return Ok(());
        }
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
        let destination = self.root.join("backups").join(id).join(stamp);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create the backup folder: {error}"))?;
        }
        copy_dir(&source, &destination)
    }

    fn install_copy_from_library(&self, id: &str, version: &str) -> Result<(), String> {
        let source = self.version_dir(id, version);
        let destination = self.installed_dir(id);
        if destination.exists() {
            remove_dir(&destination)?;
        }
        copy_dir(&source, &destination)
    }

    fn write_version(
        &self,
        id: &str,
        version: &str,
        content: &str,
        files: &[SkillFileInput],
    ) -> Result<String, String> {
        validate_id(id)?;
        validate_version(version)?;
        validate_content(content)?;
        validate_files(files)?;
        let dir = self.version_dir(id, version);
        if dir.exists() {
            return Err(format!("Version {version} of '{id}' already exists."));
        }
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("Could not create '{}': {error}", dir.display()))?;
        std::fs::write(dir.join("SKILL.md"), content)
            .map_err(|error| format!("Could not write SKILL.md: {error}"))?;
        for file in files {
            let target = safe_relative_path(&dir, &file.path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("Could not create '{}': {error}", parent.display()))?;
            }
            std::fs::write(&target, &file.content)
                .map_err(|error| format!("Could not write '{}': {error}", target.display()))?;
        }
        dir_hash(&dir)
    }

    fn read_index(&self) -> Result<Index, String> {
        let path = self.root.join("index.json");
        if !path.is_file() {
            return Ok(Index::default());
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("Could not read '{}': {error}", path.display()))?;
        if text.trim().is_empty() {
            return Ok(Index::default());
        }
        serde_json::from_str(&text).map_err(|error| {
            format!(
                "The local skill index at '{}' is corrupt: {error}",
                path.display()
            )
        })
    }

    fn write_index(&self, index: &Index) -> Result<(), String> {
        std::fs::create_dir_all(&self.root)
            .map_err(|error| format!("Could not create the skill library: {error}"))?;
        let path = self.root.join("index.json");
        let temporary = self.root.join("index.json.tmp");
        let body = serde_json::to_vec_pretty(index)
            .map_err(|error| format!("Could not serialize the skill index: {error}"))?;
        let mut file = std::fs::File::create(&temporary)
            .map_err(|error| format!("Could not write the skill index: {error}"))?;
        file.write_all(&body)
            .map_err(|error| format!("Could not write the skill index: {error}"))?;
        file.sync_all().ok();
        std::fs::rename(&temporary, &path)
            .map_err(|error| format!("Could not replace the skill index: {error}"))
    }

    fn version_dir(&self, id: &str, version: &str) -> PathBuf {
        self.root.join("library").join(id).join(version)
    }

    fn installed_dir(&self, id: &str) -> PathBuf {
        self.root.join("installed").join(id)
    }

    fn list_files(&self, dir: &Path) -> Vec<String> {
        let mut files = Vec::new();
        collect_files(dir, dir, &mut files);
        files.sort();
        files
    }
}

// ── document metadata ─────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct DocumentMeta {
    name: Option<String>,
    description: Option<String>,
    example_prompt: Option<String>,
}

/// Read the YAML frontmatter without requiring the rest of the document to be
/// YAML (it is Markdown). Keys we understand are extracted; everything else is
/// left to the skill body.
fn parse_document(content: &str) -> (DocumentMeta, String) {
    let normalized = content.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return (DocumentMeta::default(), normalized);
    };
    let Some(end) = rest.find("\n---") else {
        return (DocumentMeta::default(), normalized);
    };
    let header = &rest[..end];
    let body = rest[end + 4..].trim_start_matches('\n').to_string();

    let mut meta = DocumentMeta::default();
    if let Ok(serde_yaml::Value::Mapping(map)) = serde_yaml::from_str::<serde_yaml::Value>(header) {
        for (key, value) in map {
            let Some(key) = key.as_str() else { continue };
            let Some(value) = value.as_str().map(str::to_string) else {
                continue;
            };
            match key.trim().to_ascii_lowercase().as_str() {
                "name" => meta.name = Some(value.trim().to_string()),
                "description" => meta.description = Some(value.trim().to_string()),
                "example-prompt" | "example_prompt" => {
                    meta.example_prompt = Some(value.trim().to_string())
                }
                _ => {}
            }
        }
    }

    if meta.description.is_none() {
        meta.description = body
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| line.chars().take(240).collect());
    }
    (meta, body)
}

// ── validation and filesystem helpers ─────────────────────────────────────

fn display_name(record: &Record, id: &str) -> String {
    if record.name.trim().is_empty() {
        id.to_string()
    } else {
        record.name.clone()
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    let trimmed = id.trim();
    if trimmed.is_empty() || trimmed.len() > 80 {
        return Err("A skill id must be 1–80 characters.".to_string());
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(
            "A skill id may contain only ASCII letters, digits, '-', '_' and '.'.".to_string(),
        );
    }
    if trimmed == "." || trimmed == ".." || trimmed.contains("..") {
        return Err("A skill id may not contain '..'.".to_string());
    }
    if trimmed.starts_with('.') {
        return Err("A skill id may not start with '.'.".to_string());
    }
    Ok(())
}

fn validate_version(version: &str) -> Result<(), String> {
    let trimmed = version.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err("A version must be 1–64 characters.".to_string());
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '+'))
    {
        return Err(
            "A version may contain only letters, digits, '.', '-', '_' and '+'.".to_string(),
        );
    }
    if trimmed.contains("..") {
        return Err("A version may not contain '..'.".to_string());
    }
    Ok(())
}

fn validate_content(content: &str) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("SKILL.md cannot be empty.".to_string());
    }
    if content.len() > 2 * 1024 * 1024 {
        return Err("SKILL.md is larger than 2 MB.".to_string());
    }
    Ok(())
}

fn validate_files(files: &[SkillFileInput]) -> Result<(), String> {
    if files.len() > 128 {
        return Err("A skill may carry at most 128 supporting files.".to_string());
    }
    for file in files {
        let path = Path::new(&file.path);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(format!(
                "Supporting file '{}' must be a relative path inside the skill folder.",
                file.path
            ));
        }
        if file.content.len() > 4 * 1024 * 1024 {
            return Err(format!(
                "Supporting file '{}' is larger than 4 MB.",
                file.path
            ));
        }
    }
    Ok(())
}

fn safe_relative_path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "Supporting file '{}' must stay inside the skill folder.",
            raw
        ));
    }
    Ok(root.join(path))
}

fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// A stable hash of every file (relative path + bytes), so a local edit to a
/// supporting file is protected exactly like an edit to `SKILL.md`.
fn dir_hash(dir: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files);
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        let bytes = std::fs::read(dir.join(&relative))
            .map_err(|error| format!("Could not hash '{}': {error}", relative))?;
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Never follow a symlink. A link is not data the library authored, and
        // following one would make its contents part of the hash or the copy.
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_files(root, &path, out);
        } else if metadata.is_file() {
            if let Ok(relative) = path.strip_prefix(root) {
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

fn copy_dir(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination)
        .map_err(|error| format!("Could not create '{}': {error}", destination.display()))?;
    let entries = std::fs::read_dir(source)
        .map_err(|error| format!("Could not read '{}': {error}", source.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("Could not inspect '{}': {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_dir(&path, &target)?;
        } else if metadata.is_file() {
            std::fs::copy(&path, &target).map_err(|error| {
                format!(
                    "Could not copy '{}' to '{}': {error}",
                    path.display(),
                    target.display()
                )
            })?;
        }
    }
    Ok(())
}

fn remove_dir(path: &Path) -> Result<(), String> {
    std::fs::remove_dir_all(path)
        .map_err(|error| format!("Could not remove '{}': {error}", path.display()))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// `0.1.0` → `0.1.1`, `2.4` → `2.4.1`, non-semver → `<value>.1`.
fn bump_patch(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    match parts.as_slice() {
        [major, minor, patch] => match patch.parse::<u64>() {
            Ok(number) => format!("{major}.{minor}.{}", number + 1),
            Err(_) => format!("{version}.1"),
        },
        [major, minor] => format!("{major}.{minor}.1"),
        [major] => format!("{major}.0.1"),
        _ => format!("{version}.1"),
    }
}

/// Turn a catalog id (`owner/name`) into a filesystem-safe local id.
pub fn local_id_from_catalog(catalog_id: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in catalog_id.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch);
        } else {
            pending_dash = true;
        }
    }
    let trimmed = out.trim_matches(['-', '.', '_']).to_string();
    if trimmed.is_empty() {
        "skill".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> (LocalSkillLibrary, PathBuf) {
        let root = std::env::temp_dir().join(format!("rb-skills-{}", uuid::Uuid::new_v4()));
        (LocalSkillLibrary::at(root.clone()), root)
    }

    fn skill(name: &str, body: &str) -> String {
        format!("---\nname: {name}\ndescription: A test skill\nexample-prompt: Use {name}\n---\n\n{body}")
    }

    #[test]
    fn create_revise_install_and_rollback() {
        let (library, root) = library();
        let first = library
            .create(
                "demo",
                &skill("Demo", "Version one."),
                &[],
                Some("first".into()),
            )
            .expect("create");
        assert_eq!(first.version, "0.1.0");
        assert_eq!(first.source, "library");
        assert!(first.installed_version.is_none());

        let second = library
            .revise("demo", "0.1.0", &skill("Demo", "Version two."), &[], None)
            .expect("revise");
        assert_eq!(second.version, "0.1.1");

        let installed = library.install("demo", "0.1.1", false).expect("install");
        assert_eq!(installed.source, "installed");
        assert_eq!(installed.version, "0.1.1");
        assert!(installed.content.contains("Version two."));
        assert!(!installed.installed_modified);

        let rolled_back = library.install("demo", "0.1.0", false).expect("rollback");
        assert_eq!(rolled_back.version, "0.1.0");
        assert!(rolled_back.content.contains("Version one."));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_stale_revision_base_is_refused() {
        let (library, root) = library();
        library
            .create("demo", &skill("Demo", "one"), &[], None)
            .expect("create");
        library
            .revise("demo", "0.1.0", &skill("Demo", "two"), &[], None)
            .expect("revise");
        let error = library
            .revise("demo", "0.1.0", &skill("Demo", "three"), &[], None)
            .expect_err("stale base must fail");
        assert!(error.contains("0.1.1"), "{error}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn modified_installed_copies_are_protected_and_backed_up_on_force() {
        let (library, root) = library();
        library
            .create("demo", &skill("Demo", "one"), &[], None)
            .expect("create");
        library
            .revise("demo", "0.1.0", &skill("Demo", "two"), &[], None)
            .expect("revise");
        library.install("demo", "0.1.0", false).expect("install");

        let installed = library.installed_dir("demo").join("SKILL.md");
        std::fs::write(&installed, skill("Demo", "my local edit")).unwrap();
        let summary = library.list().unwrap().remove(0);
        assert!(summary.installed_modified);

        let error = library
            .install("demo", "0.1.1", false)
            .expect_err("modified install must be protected");
        assert!(error.contains("local modifications"), "{error}");

        library
            .install("demo", "0.1.1", true)
            .expect("forced install");
        let backups = root.join("backups").join("demo");
        let saved = std::fs::read_dir(&backups)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        assert!(
            !saved.is_empty(),
            "a forced replacement must leave a backup"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn traversal_and_absolute_supporting_files_are_refused() {
        let (library, root) = library();
        for path in ["../escape.md", "/etc/passwd", "a/../../b"] {
            let error = library
                .create(
                    "demo",
                    &skill("Demo", "body"),
                    &[SkillFileInput {
                        path: path.to_string(),
                        content: "x".to_string(),
                    }],
                    None,
                )
                .expect_err("traversal must be refused");
            assert!(error.contains("relative path"), "{error}");
        }
        for id in ["../demo", "a/b", "..", ".hidden"] {
            assert!(library
                .create(id, &skill("Demo", "body"), &[], None)
                .is_err());
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn supporting_files_are_copied_and_hashed() {
        let (library, root) = library();
        library
            .create(
                "demo",
                &skill("Demo", "body"),
                &[SkillFileInput {
                    path: "references/style.md".to_string(),
                    content: "Use short sentences.".to_string(),
                }],
                None,
            )
            .expect("create");
        let installed = library.install("demo", "0.1.0", false).expect("install");
        assert!(installed.files.contains(&"references/style.md".to_string()));
        assert!(library
            .installed_dir("demo")
            .join("references/style.md")
            .is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn frontmatter_is_parsed_and_visible_in_the_summary() {
        let (library, root) = library();
        library
            .create("demo", &skill("Pretty Name", "body"), &[], None)
            .expect("create");
        let summary = library.list().unwrap().remove(0);
        assert_eq!(summary.name, "Pretty Name");
        assert_eq!(summary.description, "A test skill");
        assert_eq!(summary.example_prompt.as_deref(), Some("Use Pretty Name"));
        let view = library.read("demo", None).expect("read");
        assert!(view.content.starts_with("---"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn marketplace_import_creates_refuses_and_updates_with_permission() {
        let (library, root) = library();
        let first = library
            .import_document(ImportSkillInput {
                id: "owner-demo".to_string(),
                name: None,
                description: None,
                content: skill("Demo", "catalog one"),
                source: "voltagent".to_string(),
                update: false,
            })
            .expect("first import");
        assert_eq!(first.version, "1.0.0");
        assert_eq!(first.source, "installed");

        let collision = library
            .import_document(ImportSkillInput {
                id: "owner-demo".to_string(),
                name: None,
                description: None,
                content: skill("Demo", "catalog two"),
                source: "voltagent".to_string(),
                update: false,
            })
            .expect_err("collision without update must fail");
        assert!(collision.contains("update=true"), "{collision}");

        let updated = library
            .import_document(ImportSkillInput {
                id: "owner-demo".to_string(),
                name: None,
                description: None,
                content: skill("Demo", "catalog two"),
                source: "voltagent".to_string(),
                update: true,
            })
            .expect("update");
        assert_eq!(updated.version, "1.0.1");
        assert!(updated.content.contains("catalog two"));

        let unchanged = library
            .import_document(ImportSkillInput {
                id: "owner-demo".to_string(),
                name: None,
                description: None,
                content: skill("Demo", "catalog two"),
                source: "voltagent".to_string(),
                update: true,
            })
            .expect("unchanged update");
        assert_eq!(unchanged.version, "1.0.1");
        assert_eq!(library.list().unwrap().remove(0).versions.len(), 2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn create_with_source_records_provenance_and_leaves_create_local() {
        let (library, root) = library();
        let learned = library
            .create_with_source(
                "learned-demo",
                &skill("Learned", "learned body"),
                &[],
                Some("Learned from run abc".into()),
                "learned:run:abc",
            )
            .expect("create with source");
        assert_eq!(learned.provenance.as_deref(), Some("learned:run:abc"));

        let summary = library
            .list()
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == "learned-demo")
            .expect("summary");
        assert_eq!(summary.provenance.as_deref(), Some("learned:run:abc"));

        library
            .create("plain-demo", &skill("Plain", "plain body"), &[], None)
            .expect("plain create");
        let plain = library.read("plain-demo", None).expect("read plain");
        assert_eq!(plain.provenance.as_deref(), Some("local"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn create_with_source_refuses_a_duplicate_id() {
        let (library, root) = library();
        library
            .create_with_source("demo", &skill("Demo", "one"), &[], None, "learned:run:1")
            .expect("first");
        let error = library
            .create_with_source("demo", &skill("Demo", "two"), &[], None, "learned:run:2")
            .expect_err("duplicate");
        assert!(error.contains("already exists"), "{error}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn catalog_ids_become_safe_local_ids() {
        assert_eq!(local_id_from_catalog("anthropics/docx"), "anthropics-docx");
        assert_eq!(
            local_id_from_catalog("Owner Name/Skill!"),
            "owner-name-skill"
        );
        assert_eq!(local_id_from_catalog("***"), "skill");
    }
}
