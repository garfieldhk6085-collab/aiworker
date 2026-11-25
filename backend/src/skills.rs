use std::path::{Path, PathBuf};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDetail {
    pub name: String,
    pub description: String,
    pub content: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertSkillRequest {
    pub name: String,
    pub description: String,
    pub content: String,
}

fn validate_skill_name(name: &str) -> anyhow::Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(anyhow!("skill name must be 1..=64 characters"));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(anyhow!(
            "skill name may only contain ASCII letters, digits, '-' and '_'"
        ));
    }
    Ok(trimmed.to_string())
}

fn skill_dir(skills_root: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let safe = validate_skill_name(name)?;
    Ok(skills_root.join(safe))
}

pub async fn list_skills(skills_root: &Path) -> anyhow::Result<Vec<SkillSummary>> {
    if !skills_root.exists() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let mut rd = tokio::fs::read_dir(skills_root).await?;
    while let Some(entry) = rd.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let skill_md = entry.path().join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let raw = tokio::fs::read_to_string(&skill_md)
            .await
            .unwrap_or_default();
        let (parsed_name, description, _) = parse_frontmatter(&raw);
        let files = list_companion_files(&entry.path()).await;
        out.push(SkillSummary {
            name: parsed_name.unwrap_or(dir_name),
            description: description.unwrap_or_else(|| "Custom skill".to_string()),
            files,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub async fn get_skill(skills_root: &Path, name: &str) -> anyhow::Result<SkillDetail> {
    let dir = skill_dir(skills_root, name)?;
    let skill_md = dir.join("SKILL.md");
    if !skill_md.exists() {
        return Err(anyhow!("skill '{name}' not found"));
    }
    let raw = tokio::fs::read_to_string(&skill_md).await?;
    let (parsed_name, description, body) = parse_frontmatter(&raw);
    let files = list_companion_files(&dir).await;

    Ok(SkillDetail {
        name: parsed_name.unwrap_or_else(|| name.to_string()),
        description: description.unwrap_or_default(),
        content: body,
        files,
    })
}

pub async fn upsert_skill(
    skills_root: &Path,
    req: &UpsertSkillRequest,
) -> anyhow::Result<SkillDetail> {
    let safe_name = validate_skill_name(&req.name)?;
    let dir = skills_root.join(&safe_name);
    tokio::fs::create_dir_all(&dir).await?;

    let desc = req.description.trim().replace('\n', " ");
    let full_md = if req.content.trim_start().starts_with("---") {
        req.content.clone()
    } else {
        format!(
            "---\nname: {safe_name}\ndescription: {desc}\n---\n\n{}\n",
            req.content.trim()
        )
    };

    tokio::fs::write(dir.join("SKILL.md"), full_md).await?;
    get_skill(skills_root, &safe_name).await
}

pub async fn delete_skill(skills_root: &Path, name: &str) -> anyhow::Result<()> {
    let dir = skill_dir(skills_root, name)?;
    if !dir.exists() {
        return Err(anyhow!("skill '{name}' not found"));
    }
    tokio::fs::remove_dir_all(dir).await?;
    Ok(())
}

async fn list_companion_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if name != "SKILL.md" {
                files.push(name);
            }
        }
    }
    files.sort();
    files
}

fn parse_frontmatter(raw: &str) -> (Option<String>, Option<String>, String) {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (None, None, raw.to_string());
    }
    let after_first = &trimmed[3..];
    let Some(end_idx) = after_first.find("\n---") else {
        return (None, None, raw.to_string());
    };
    let header = &after_first[..end_idx];
    let body = after_first[end_idx + 4..].trim_start().to_string();

    let mut name = None;
    let mut description = None;
    for line in header.lines() {
        if let Some(rest) = line.strip_prefix("name:") {
            name = Some(rest.trim().trim_matches('"').trim_matches('\'').to_string());
        } else if let Some(rest) = line.strip_prefix("description:") {
            description = Some(rest.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }

    (name, description, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_lists_and_reads_skill() {
        let dir = std::env::temp_dir().join(format!("aiworker_skills_{}", uuid::Uuid::new_v4()));
        let req = UpsertSkillRequest {
            name: "daily_brief".to_string(),
            description: "Generate a daily summary from workspace notes".to_string(),
            content: "# Steps\n1. Read notes\n2. Summarize".to_string(),
        };
        let saved = upsert_skill(&dir, &req).await.unwrap();
        assert_eq!(saved.name, "daily_brief");

        let all = list_skills(&dir).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].description, "Generate a daily summary from workspace notes");

        let loaded = get_skill(&dir, "daily_brief").await.unwrap();
        assert!(loaded.content.contains("Summarize"));

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
