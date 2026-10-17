use crate::{
    gh::{Cancellation, GhSkillGateway},
    service::StateRepository,
};
use tasc_core::{
    discovery::{Capabilities, Diagnostic, Preview, RankedSkill, SearchHit, SkillSource},
    LOCAL_STATE_VERSION,
};

pub struct DiscoveryService<G, R> {
    gateway: G,
    repository: R,
}

impl<G: GhSkillGateway, R: StateRepository> DiscoveryService<G, R> {
    pub fn new(gateway: G, repository: R) -> Self {
        Self {
            gateway,
            repository,
        }
    }

    fn state(&self) -> Result<tasc_core::LocalState, Diagnostic> {
        let state = self.repository.load().map_err(state_error)?;
        if state.version != LOCAL_STATE_VERSION {
            return Err(state_error(format!(
                "unsupported Local State version {}",
                state.version
            )));
        }
        Ok(state)
    }

    pub fn sources(&self) -> Result<Vec<SkillSource>, Diagnostic> {
        let mut sources = self.state()?.sources;
        sources.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(sources)
    }

    pub fn source(&self, selector: &str) -> Result<SkillSource, Diagnostic> {
        let source = normalize_source(selector)?;
        self.sources()?
            .into_iter()
            .find(|item| item.id == source.id)
            .ok_or_else(|| {
                usage(
                    "source_not_found",
                    "Source is not recorded.",
                    "Use `tasc source add <owner/repo> --yes`.",
                )
            })
    }

    pub fn add_source(
        &self,
        locator: &str,
        dry_run: bool,
        approved: bool,
    ) -> Result<SkillSource, Diagnostic> {
        let source = normalize_source(locator)?;
        let mut state = self.state()?;
        if !state.sources.iter().any(|item| item.id == source.id) && !dry_run {
            if !approved {
                return Err(usage(
                    "confirmation_required",
                    "Recording a source requires approval.",
                    "Pass --yes or inspect with --dry-run.",
                ));
            }
            state.sources.push(source.clone());
            state.sources.sort_by(|a, b| a.id.cmp(&b.id));
            self.repository.save(&state).map_err(state_error)?;
        }
        Ok(source)
    }

    pub async fn capabilities(&self, cancel: &Cancellation) -> Result<Capabilities, Diagnostic> {
        self.gateway.capabilities(cancel).await
    }

    pub async fn search(
        &self,
        query: &str,
        cancel: &Cancellation,
    ) -> Result<Vec<RankedSkill>, Diagnostic> {
        let query = query.trim();
        if query.chars().count() < 2 || query.len() > 4096 || query.chars().any(char::is_control) {
            return Err(usage(
                "invalid_query",
                "Search query must contain 2–4096 characters without control characters.",
                "Supply a skill name or keyword.",
            ));
        }
        rank(query, self.gateway.search(query, cancel).await?)
    }

    pub async fn preview(
        &self,
        selector: &str,
        source: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<Preview, Diagnostic> {
        let (source, path) = if let Some(identity) =
            selector.strip_prefix("skill:https://github.com/")
        {
            let parts: Vec<_> = identity.splitn(3, '/').collect();
            if parts.len() != 3 {
                return Err(usage(
                    "invalid_selector",
                    "Invalid Skill Identity.",
                    "Use an ID returned by search.",
                ));
            }
            let identity_source = normalize_source(&format!("{}/{}", parts[0], parts[1]))?;
            if let Some(source) = source {
                if normalize_source(source)? != identity_source {
                    return Err(usage(
                        "source_mismatch",
                        "Skill Identity does not belong to the selected source.",
                        "Use the source returned by search.",
                    ));
                }
            }
            (identity_source, parts[2].to_string())
        } else if let Some(source) =
            source.filter(|_| selector.ends_with("/SKILL.md") || selector == "SKILL.md")
        {
            (normalize_source(source)?, selector.to_string())
        } else {
            let source = source.map(normalize_source).transpose()?;
            let matches: Vec<_> = self
                .search(selector, cancel)
                .await?
                .into_iter()
                .filter(|item| {
                    source.as_ref().is_none_or(|source| {
                        source.locator == format!("https://github.com/{}", item.skill.repo)
                    })
                })
                .filter(|item| {
                    item.skill.skill_name == selector
                        || item.skill.path == selector
                        || format!("{}/{}", item.skill.namespace, item.skill.skill_name) == selector
                })
                .collect();
            if matches.len() != 1 {
                let mut error = usage(
                    if matches.is_empty() { "skill_not_found" } else { "ambiguous_selection" },
                    "Selector must identify exactly one skill.",
                    "Use a full Skill Identity from search or an exact SKILL.md path with --source.",
                );
                error.candidates = matches.into_iter().map(|item| item.id).collect();
                error.candidates.sort();
                return Err(error);
            }
            let hit = &matches[0].skill;
            (normalize_source(&hit.repo)?, hit.path.clone())
        };
        validate_path(&path)?;
        let repository = source.locator.trim_start_matches("https://github.com/");
        let rendered_text = self.gateway.preview(repository, &path, cancel).await?;
        Ok(Preview {
            source,
            path,
            rendered_text,
        })
    }
}

pub fn normalize_source(locator: &str) -> Result<SkillSource, Diagnostic> {
    let repo = locator.strip_prefix("source:").unwrap_or(locator);
    let repo = repo
        .strip_prefix("https://github.com/")
        .unwrap_or(repo)
        .trim_end_matches('/');
    let repo = repo
        .strip_suffix(".git")
        .unwrap_or(repo)
        .to_ascii_lowercase();
    let parts: Vec<_> = repo.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-._".contains(&byte))
        })
    {
        return Err(usage(
            "invalid_source",
            "Source must be a GitHub owner/repository or HTTPS GitHub URL without credentials.",
            "Use owner/repository.",
        ));
    }
    let locator = format!("https://github.com/{repo}");
    Ok(SkillSource {
        id: format!("source:{locator}"),
        locator,
    })
}

fn validate_path(path: &str) -> Result<(), Diagnostic> {
    if path.rsplit('/').next() != Some("SKILL.md")
        || path.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part
                    .chars()
                    .any(|character| character.is_control() || matches!(character, '\\' | '@'))
        })
    {
        return Err(usage(
            "invalid_skill_path",
            "Expected a repository-relative SKILL.md path.",
            "Use the exact path returned by search.",
        ));
    }
    Ok(())
}

fn rank(query: &str, hits: Vec<SearchHit>) -> Result<Vec<RankedSkill>, Diagnostic> {
    let query = query.to_lowercase();
    let mut results = Vec::new();
    for mut hit in hits {
        let source = normalize_source(&hit.repo).map_err(|_| malformed())?;
        validate_path(&hit.path).map_err(|_| malformed())?;
        if hit.skill_name.trim().is_empty() {
            return Err(malformed());
        }
        hit.repo = source
            .locator
            .trim_start_matches("https://github.com/")
            .into();
        let name = hit.skill_name.to_lowercase();
        let reason = if name == query {
            "exact name match"
        } else if name.contains(&query) {
            "name contains query"
        } else if hit.description.to_lowercase().contains(&query) {
            "description contains query"
        } else {
            "delegated search match"
        };
        results.push(RankedSkill {
            id: format!("skill:{}/{}", source.locator, hit.path),
            rank: 0,
            reason: reason.into(),
            skill: hit,
        });
    }
    let relevance = |item: &RankedSkill| match item.reason.as_str() {
        "exact name match" => 0,
        "name contains query" => 1,
        "description contains query" => 2,
        _ => 3,
    };
    results.sort_by(|a, b| {
        relevance(a)
            .cmp(&relevance(b))
            .then_with(|| b.skill.stars.cmp(&a.skill.stars))
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.skill.skill_name.cmp(&b.skill.skill_name))
            .then_with(|| a.skill.namespace.cmp(&b.skill.namespace))
            .then_with(|| a.skill.description.cmp(&b.skill.description))
    });
    let mut seen = std::collections::BTreeSet::new();
    results.retain(|item| seen.insert(item.id.clone()));
    for (index, item) in results.iter_mut().enumerate() {
        item.rank = index + 1;
    }
    Ok(results)
}

fn malformed() -> Diagnostic {
    Diagnostic::new(
        "gh_malformed_output",
        "operational",
        "gh",
        "Search returned invalid skill metadata.",
        "Upgrade GitHub CLI and retry.",
    )
}

fn state_error(message: String) -> Diagnostic {
    Diagnostic::new(
        "local_state_failed",
        "operational",
        "filesystem",
        &message,
        "Inspect Local State without overwriting it.",
    )
}

fn usage(code: &str, message: &str, remediation: &str) -> Diagnostic {
    Diagnostic::new(code, "usage", "tasc", message, remediation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    use tasc_core::LocalState;

    #[derive(Clone, Default)]
    struct Repository(Rc<RefCell<LocalState>>);

    impl StateRepository for Repository {
        fn load(&self) -> Result<LocalState, String> {
            Ok(self.0.borrow().clone())
        }
        fn save(&self, state: &LocalState) -> Result<(), String> {
            *self.0.borrow_mut() = state.clone();
            Ok(())
        }
    }

    struct Gateway(Vec<SearchHit>, RefCell<Vec<String>>);

    impl GhSkillGateway for Gateway {
        async fn capabilities(&self, _: &Cancellation) -> Result<Capabilities, Diagnostic> {
            Ok(Capabilities {
                version: "fixture".into(),
                search: true,
                preview: true,
            })
        }
        async fn search(&self, _: &str, _: &Cancellation) -> Result<Vec<SearchHit>, Diagnostic> {
            Ok(self.0.clone())
        }
        async fn preview(
            &self,
            repository: &str,
            path: &str,
            _: &Cancellation,
        ) -> Result<String, Diagnostic> {
            self.1.borrow_mut().push(format!("{repository}/{path}"));
            Ok("unparsed upstream content".into())
        }
    }

    fn hits() -> Vec<SearchHit> {
        serde_json::from_str(include_str!("../tests/fixtures/search.json")).unwrap()
    }

    #[test]
    fn source_metadata_is_normalized_idempotent_and_requires_approval() {
        let repository = Repository::default();
        let service =
            DiscoveryService::new(Gateway(Vec::new(), RefCell::default()), repository.clone());
        let source = service
            .add_source("https://github.com/ALPHA/Skills.git/", true, false)
            .unwrap();
        assert!(service.sources().unwrap().is_empty());
        assert_eq!(
            service
                .add_source("alpha/skills", false, false)
                .unwrap_err()
                .code,
            "confirmation_required"
        );
        assert!(service.sources().unwrap().is_empty());
        assert_eq!(
            service.add_source("alpha/skills", false, true).unwrap(),
            source
        );
        assert_eq!(
            service.add_source(&source.id, false, false).unwrap(),
            source
        );
        assert_eq!(service.source("ALPHA/SKILLS").unwrap(), source);
        assert_eq!(service.sources().unwrap(), vec![source]);
        assert!(repository.0.borrow().installations.is_empty());
        assert!(repository.0.borrow().identities.is_empty());
    }

    #[test]
    fn source_validation_rejects_credentials_options_and_non_github_hosts() {
        for locator in [
            "******github.com/a/b",
            "--help",
            "https://example.org/a/b",
            "a/../b",
            "a/b?token=value",
            "/a/b",
            "a/b\n",
        ] {
            assert_eq!(
                normalize_source(locator).unwrap_err().code,
                "invalid_source"
            );
        }
    }

    #[test]
    fn skill_paths_allow_unicode_but_cannot_select_a_different_file_or_revision() {
        assert!(validate_path("skills/révision/SKILL.md").is_ok());
        for path in [
            "skills/NOTSKILL.md",
            "skills/example/SKILL.md@main",
            "skills/example\\SKILL.md",
        ] {
            assert!(validate_path(path).is_err());
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ranking_deduplication_and_ambiguity_are_independent_of_upstream_order() {
        let mut original = hits();
        original.push(original[0].clone());
        let expected = rank("review", original.clone()).unwrap();
        original.reverse();
        assert_eq!(rank("review", original.clone()).unwrap(), expected);
        assert_eq!(expected.len(), 4);
        assert_eq!(expected[0].skill.repo, "alpha/skills");
        assert_eq!(expected[1].skill.repo, "beta/skills");
        assert_eq!(expected[2].reason, "name contains query");
        assert_eq!(expected[3].reason, "description contains query");
        let service =
            DiscoveryService::new(Gateway(original, RefCell::default()), Repository::default());
        let cancel = Cancellation::default();
        let error = service.preview("review", None, &cancel).await.unwrap_err();
        assert_eq!(error.code, "ambiguous_selection");
        assert_eq!(
            error.candidates,
            vec![expected[0].id.clone(), expected[1].id.clone()]
        );
        assert!(service.gateway.1.borrow().is_empty());
        let preview = service
            .preview("review", Some("alpha/skills"), &cancel)
            .await
            .unwrap();
        assert_eq!(preview.path, "skills/review/SKILL.md");
        let exact = service
            .preview(&expected[0].id, None, &cancel)
            .await
            .unwrap();
        assert_eq!(preview, exact);
        assert_eq!(exact.rendered_text, "unparsed upstream content");
        assert_eq!(
            service
                .preview(&expected[0].id, Some("beta/skills"), &cancel)
                .await
                .unwrap_err()
                .code,
            "source_mismatch"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn empty_search_is_valid_but_invalid_metadata_and_selectors_are_not() {
        let service = DiscoveryService::new(
            Gateway(Vec::new(), RefCell::default()),
            Repository::default(),
        );
        let cancel = Cancellation::default();
        assert!(service.search("review", &cancel).await.unwrap().is_empty());
        assert_eq!(
            service.search(" ", &cancel).await.unwrap_err().code,
            "invalid_query"
        );
        assert_eq!(
            service
                .preview("review", None, &cancel)
                .await
                .unwrap_err()
                .code,
            "skill_not_found"
        );
        for path in [
            "../SKILL.md",
            "/SKILL.md",
            "skills/../SKILL.md",
            "skill@ref/SKILL.md",
        ] {
            assert!(service
                .preview(path, Some("alpha/skills"), &cancel)
                .await
                .is_err());
        }
        assert!(service.gateway.1.borrow().is_empty());
        let mut invalid = hits();
        invalid[0].repo = "https://credentials@example.org/repo".into();
        assert_eq!(
            rank("review", invalid).unwrap_err().code,
            "gh_malformed_output"
        );
    }

    #[test]
    fn old_state_loads_with_no_sources_and_future_state_is_not_modified() {
        let old: LocalState = serde_json::from_str(
            r#"{"version":1,"identities":[],"targets":[],"installations":[]}"#,
        )
        .unwrap();
        assert!(old.sources.is_empty());
        let repository = Repository::default();
        repository.0.borrow_mut().version = 99;
        let service =
            DiscoveryService::new(Gateway(Vec::new(), RefCell::default()), repository.clone());
        assert_eq!(
            service
                .add_source("alpha/skills", false, true)
                .unwrap_err()
                .code,
            "local_state_failed"
        );
        assert_eq!(repository.0.borrow().version, 99);
        assert!(repository.0.borrow().sources.is_empty());
    }
}
