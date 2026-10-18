use crate::{
    gh::{Cancellation, GhSkillGateway},
    service::{DiscoveredSkill as LocalSkill, StateRepository},
};
use tasc_core::{
    discovery::{
        Capabilities, Diagnostic, DiscoverResult, DiscoveredSkill, Preview, RankedSkill,
        RemoteExpansionStatus, RerankingStatus, SearchHit, SkillSource,
    },
    ProvenanceConfidence, ProvenanceEvidence, ProvenanceKind, SkillIdentity, LOCAL_STATE_VERSION,
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

    pub fn discoverable_sources(&self) -> Result<Vec<SkillSource>, Diagnostic> {
        let state = self.state()?;
        let mut sources = state.sources;
        sources.extend(
            state
                .watchlists
                .into_iter()
                .map(|watchlist| watchlist.source),
        );
        sources.sort_by(|a, b| a.id.cmp(&b.id));
        sources.dedup_by(|a, b| a.id == b.id);
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
        rank_search(query, self.gateway.search(query, cancel).await?)
    }

    pub async fn discover(
        &self,
        query: &str,
        local: Vec<LocalSkill>,
        remote: bool,
        cancel: &Cancellation,
    ) -> Result<DiscoverResult, Diagnostic> {
        let query = query.trim();
        if query.chars().count() < 2 || query.len() > 4096 || query.chars().any(char::is_control) {
            return Err(usage(
                "invalid_query",
                "Discover query must contain 2–4096 characters without control characters.",
                "Describe the skill or capability you are looking for.",
            ));
        }

        let mut candidates = local
            .into_iter()
            .map(local_candidate)
            .collect::<Result<Vec<_>, _>>()?;
        let mut diagnostics = Vec::new();
        let mut remote_expansion = RemoteExpansionStatus::Complete;
        if remote {
            match self.gateway.search(query, cancel).await {
                Ok(hits) => candidates.extend(remote_candidates(hits)?),
                Err(diagnostic) => {
                    remote_expansion = RemoteExpansionStatus::Unavailable;
                    diagnostics.push(diagnostic);
                }
            }
        } else {
            remote_expansion = RemoteExpansionStatus::Unavailable;
            diagnostics.push(Diagnostic::new(
                "remote_expansion_unavailable",
                "dependency",
                "tasc",
                "Remote skill sources were not searched because offline mode is enabled.",
                "Remove --offline to include GitHub sources; local results are still available.",
            ));
        }

        let sources = self.discoverable_sources()?;
        let mut items = rank_discover(query, candidates);
        for (index, item) in items.iter_mut().enumerate() {
            item.rank = index + 1;
        }
        let mut result = DiscoverResult {
            query: query.to_string(),
            items,
            eligible_sources: sources.clone(),
            remote_expansion,
            reranking: RerankingStatus::NotRequested,
            diagnostics,
        };
        if result.items.is_empty() && !sources.is_empty() {
            result.diagnostics.push(Diagnostic::new(
                "no_matching_skills",
                "finding",
                "tasc",
                "No eligible skill matched the Discover query.",
                "Try a different intent, category, or capability.",
            ));
        }
        Ok(result)
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
            let name = selector.strip_suffix("/SKILL.md").unwrap_or(selector);
            let query = name.rsplit('/').next().unwrap_or(name);
            let matches: Vec<_> = self
                .search(query, cancel)
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

fn rank_search(query: &str, hits: Vec<SearchHit>) -> Result<Vec<RankedSkill>, Diagnostic> {
    let exact_query = query.trim();
    let query = exact_query.to_lowercase();
    let query_terms = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
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
        let full_name = if hit.namespace.is_empty() {
            hit.skill_name.clone()
        } else {
            format!("{}/{}", hit.namespace, hit.skill_name)
        };
        let direct_match = hit.skill_name.eq_ignore_ascii_case(&query)
            || full_name.eq_ignore_ascii_case(&query)
            || hit.path == exact_query
            || format!("skill:{}/{}", source.locator, hit.path) == exact_query;
        let searchable_terms = terms(&format!("{} {}", hit.description, hit.repo));
        let keyword_match = !query_terms.is_empty()
            && query_terms
                .iter()
                .all(|term| searchable_terms.contains(term));
        if !direct_match && !keyword_match {
            continue;
        }
        let reason = if hit.skill_name.eq_ignore_ascii_case(&query)
            || full_name.eq_ignore_ascii_case(&query)
        {
            "exact name match"
        } else if hit.path == exact_query
            || format!("skill:{}/{}", source.locator, hit.path) == exact_query
        {
            "exact path match"
        } else {
            "exact keyword match"
        };
        results.push(RankedSkill {
            id: format!("skill:{}/{}", source.locator, hit.path),
            rank: 0,
            reason: reason.into(),
            skill: hit,
        });
    }
    results.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.skill.skill_name.cmp(&b.skill.skill_name))
            .then_with(|| a.skill.namespace.cmp(&b.skill.namespace))
            .then_with(|| a.skill.description.cmp(&b.skill.description))
            .then_with(|| b.skill.stars.cmp(&a.skill.stars))
    });
    let mut seen = std::collections::BTreeSet::new();
    results.retain(|item| seen.insert(item.id.clone()));
    for (index, item) in results.iter_mut().enumerate() {
        item.rank = index + 1;
    }
    Ok(results)
}

#[derive(Clone)]
struct DiscoverCandidate {
    item: DiscoveredSkill,
    score: usize,
    source_priority: u8,
    provenance_priority: u8,
}

fn local_candidate(skill: LocalSkill) -> Result<DiscoverCandidate, Diagnostic> {
    let contents = std::fs::read_to_string(std::path::Path::new(&skill.path).join("SKILL.md"))
        .map_err(|error| state_error(format!("cannot read discovered skill metadata: {error}")))?;
    let source = skill.identity.source.clone();
    let provenance = provenance(&source);
    Ok(DiscoverCandidate {
        item: DiscoveredSkill {
            id: skill.identity.id.clone(),
            rank: 0,
            model_rank: None,
            identity: skill.identity,
            source: SkillSource {
                id: format!("source:{source}"),
                locator: source,
            },
            description: skill.description,
            categories: metadata_values(&contents, "categories"),
            capabilities: metadata_values(&contents, "capabilities"),
            provenance: provenance.clone(),
            match_reasons: Vec::new(),
        },
        score: 0,
        source_priority: 0,
        provenance_priority: provenance_priority(provenance.confidence),
    })
}

fn remote_candidates(hits: Vec<SearchHit>) -> Result<Vec<DiscoverCandidate>, Diagnostic> {
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
        let identity = SkillIdentity {
            id: format!("skill:{}/{}", source.locator, hit.path),
            name: if hit.namespace.is_empty() {
                hit.skill_name.clone()
            } else {
                format!("{}/{}", hit.namespace, hit.skill_name)
            },
            source: source.locator.clone(),
            relative_path: hit.path.clone(),
        };
        let provenance = ProvenanceEvidence {
            kind: ProvenanceKind::GithubRepository,
            confidence: ProvenanceConfidence::Probable,
            raw_evidence: Some(source.locator.clone()),
            explanation: "The GitHub search result identifies this repository as the skill source."
                .to_string(),
        };
        results.push(DiscoverCandidate {
            item: DiscoveredSkill {
                id: identity.id.clone(),
                rank: 0,
                model_rank: None,
                identity,
                source,
                description: hit.description,
                categories: Vec::new(),
                capabilities: Vec::new(),
                provenance: provenance.clone(),
                match_reasons: Vec::new(),
            },
            score: 0,
            source_priority: 1,
            provenance_priority: provenance_priority(provenance.confidence),
        });
    }
    Ok(results)
}

fn rank_discover(query: &str, mut candidates: Vec<DiscoverCandidate>) -> Vec<DiscoveredSkill> {
    let normalized_query = query.to_lowercase();
    let (category_query, capability_query, query) =
        if let Some(value) = normalized_query.strip_prefix("category:") {
            (Some(value.trim().to_string()), None, String::new())
        } else if let Some(value) = normalized_query.strip_prefix("capability:") {
            (None, Some(value.trim().to_string()), String::new())
        } else {
            (None, None, normalized_query)
        };
    let query_terms = terms(&query);
    candidates.retain_mut(|candidate| {
        let name = candidate.item.identity.name.to_lowercase();
        let description = candidate.item.description.to_lowercase();
        let categories = candidate
            .item
            .categories
            .iter()
            .map(|value| value.to_lowercase())
            .collect::<Vec<_>>();
        let capabilities = candidate
            .item
            .capabilities
            .iter()
            .map(|value| value.to_lowercase())
            .collect::<Vec<_>>();
        let mut score = 0usize;
        let mut reasons = Vec::new();
        if category_query.is_some() {
            if categories
                .iter()
                .any(|category| category == category_query.as_ref().unwrap())
            {
                score += 80;
                reasons.push(format!(
                    "category match: {}",
                    category_query.as_ref().unwrap()
                ));
            } else {
                return false;
            }
        }
        if capability_query.is_some() {
            if capabilities
                .iter()
                .any(|capability| capability == capability_query.as_ref().unwrap())
            {
                score += 80;
                reasons.push(format!(
                    "capability match: {}",
                    capability_query.as_ref().unwrap()
                ));
            } else {
                return false;
            }
        }
        if !query.is_empty() {
            if name == query {
                score += 100;
                reasons.push("exact name match".to_string());
            } else if name.contains(&query) {
                score += 75;
                reasons.push("name contains intent".to_string());
            } else {
                let name_matches = query_terms
                    .iter()
                    .filter(|term| terms(&name).contains(term))
                    .count();
                if name_matches > 0 {
                    score += 50 + name_matches * 5;
                    reasons.push(format!("name keyword matches: {name_matches}"));
                }
            }
            if description.contains(&query) {
                score += 35;
                reasons.push("description contains intent".to_string());
            }
            let metadata_terms = terms(&format!(
                "{} {} {}",
                candidate.item.description,
                candidate.item.categories.join(" "),
                candidate.item.capabilities.join(" ")
            ));
            let matched_terms = query_terms
                .iter()
                .filter(|term| metadata_terms.contains(term))
                .count();
            if matched_terms > 0 {
                score += matched_terms * 8;
                reasons.push(format!("metadata keyword matches: {matched_terms}"));
            }
            let category_matches = categories.iter().filter(|category| {
                query_terms
                    .iter()
                    .any(|term| terms(category).contains(term))
            });
            for category in category_matches {
                score += 15;
                reasons.push(format!("category evidence: {category}"));
            }
            let capability_matches = capabilities.iter().filter(|capability| {
                query_terms
                    .iter()
                    .any(|term| terms(capability).contains(term))
            });
            for capability in capability_matches {
                score += 15;
                reasons.push(format!("capability evidence: {capability}"));
            }
        }
        if score == 0 {
            return false;
        }
        reasons.push(format!(
            "provenance evidence: {} ({:?})",
            provenance_name(candidate.item.provenance.kind),
            candidate.item.provenance.confidence
        ));
        candidate.item.match_reasons = reasons;
        candidate.score = score;
        true
    });
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.provenance_priority.cmp(&left.provenance_priority))
            .then_with(|| left.source_priority.cmp(&right.source_priority))
            .then_with(|| left.item.id.cmp(&right.item.id))
            .then_with(|| left.item.identity.name.cmp(&right.item.identity.name))
            .then_with(|| left.item.description.cmp(&right.item.description))
            .then_with(|| left.item.source.id.cmp(&right.item.source.id))
            .then_with(|| left.item.categories.cmp(&right.item.categories))
            .then_with(|| left.item.capabilities.cmp(&right.item.capabilities))
            .then_with(|| {
                left.item
                    .provenance
                    .raw_evidence
                    .cmp(&right.item.provenance.raw_evidence)
            })
            .then_with(|| {
                left.item
                    .provenance
                    .explanation
                    .cmp(&right.item.provenance.explanation)
            })
    });
    let mut seen = std::collections::BTreeSet::new();
    candidates
        .into_iter()
        .filter_map(|candidate| {
            seen.insert(candidate.item.id.clone())
                .then_some(candidate.item)
        })
        .collect()
}

pub fn apply_model_ranking(
    result: &mut DiscoverResult,
    ordered_ids: &[String],
) -> Result<(), Diagnostic> {
    let known = result
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let proposed = ordered_ids
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if ordered_ids.len() != result.items.len()
        || proposed.len() != ordered_ids.len()
        || known != proposed
    {
        return Err(usage(
            "invalid_rerank_output",
            "Model reranking must return every candidate identity exactly once.",
            "Retry without --rerank or inspect the Copilot CLI response.",
        ));
    }
    let ranks = ordered_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index + 1))
        .collect::<std::collections::BTreeMap<_, _>>();
    for item in &mut result.items {
        item.model_rank = ranks.get(item.id.as_str()).copied();
    }
    result
        .items
        .sort_by_key(|item| item.model_rank.unwrap_or(usize::MAX));
    result.reranking = RerankingStatus::Applied;
    Ok(())
}

fn terms(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn metadata_values(contents: &str, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut lines = contents.lines();
    if lines.next().map(str::trim) != Some("---") {
        return values;
    }
    let mut in_metadata = false;
    let mut list_indent = None;
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        let indent = line.len() - line.trim_start().len();
        if trimmed == "metadata:" && indent == 0 {
            in_metadata = true;
            continue;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            let nested = indent > 0;
            let is_key =
                name.trim() == key && ((!nested && !in_metadata) || (nested && in_metadata));
            if is_key {
                let value = value.trim();
                if !value.is_empty() {
                    values.extend(
                        value
                            .trim_matches(['[', ']'])
                            .split(',')
                            .map(|item| item.trim().trim_matches(['\'', '"']))
                            .filter(|item| !item.is_empty())
                            .map(str::to_string),
                    );
                    list_indent = None;
                } else {
                    list_indent = Some(indent);
                }
                continue;
            }
            if !nested {
                in_metadata = false;
            }
        }
        if let Some(base_indent) = list_indent {
            if indent <= base_indent && !trimmed.is_empty() {
                list_indent = None;
            } else if let Some(item) = trimmed.strip_prefix("- ") {
                values.push(item.trim().trim_matches(['\'', '"']).to_string());
            }
        }
    }
    values.sort_by_key(|value| value.to_lowercase());
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    values
}

fn provenance(source: &str) -> ProvenanceEvidence {
    if source.starts_with("https://github.com/") {
        ProvenanceEvidence {
            kind: ProvenanceKind::GithubRepository,
            confidence: ProvenanceConfidence::Probable,
            raw_evidence: Some(source.to_string()),
            explanation: "The skill metadata declares a GitHub repository as its source."
                .to_string(),
        }
    } else if source.starts_with("file:") {
        ProvenanceEvidence {
            kind: ProvenanceKind::LocalWorkspace,
            confidence: ProvenanceConfidence::Inferred,
            raw_evidence: Some(source.to_string()),
            explanation: "A local file source indicates a workspace skill.".to_string(),
        }
    } else {
        ProvenanceEvidence::default()
    }
}

fn provenance_priority(confidence: ProvenanceConfidence) -> u8 {
    match confidence {
        ProvenanceConfidence::Verified => 3,
        ProvenanceConfidence::Probable => 2,
        ProvenanceConfidence::Inferred => 1,
        ProvenanceConfidence::Unavailable => 0,
    }
}

fn provenance_name(kind: ProvenanceKind) -> &'static str {
    match kind {
        ProvenanceKind::GithubRepository => "github_repository",
        ProvenanceKind::MarketplacePlugin => "marketplace_plugin",
        ProvenanceKind::LocalWorkspace => "local_workspace",
        ProvenanceKind::BundledFirstParty => "bundled_first_party",
        ProvenanceKind::Unknown => "unknown",
    }
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

    struct NamespacedGateway;

    impl GhSkillGateway for NamespacedGateway {
        async fn capabilities(&self, _: &Cancellation) -> Result<Capabilities, Diagnostic> {
            unreachable!()
        }
        async fn search(
            &self,
            query: &str,
            _: &Cancellation,
        ) -> Result<Vec<SearchHit>, Diagnostic> {
            assert_eq!(query, "review", "gh keyword filtering needs the bare name");
            let mut hit = hits().remove(1);
            hit.namespace = "author".into();
            Ok(vec![hit])
        }
        async fn preview(
            &self,
            repository: &str,
            path: &str,
            _: &Cancellation,
        ) -> Result<String, Diagnostic> {
            assert_eq!(repository, "beta/skills");
            assert_eq!(path, "skills/review/SKILL.md");
            Ok("namespaced preview".into())
        }
    }

    struct UnavailableGateway;

    impl GhSkillGateway for UnavailableGateway {
        async fn capabilities(&self, _: &Cancellation) -> Result<Capabilities, Diagnostic> {
            unreachable!()
        }
        async fn search(&self, _: &str, _: &Cancellation) -> Result<Vec<SearchHit>, Diagnostic> {
            Err(Diagnostic::new(
                "gh_authentication",
                "dependency",
                "gh",
                "GitHub authentication is unavailable.",
                "Authenticate with GitHub CLI and retry.",
            ))
        }
        async fn preview(&self, _: &str, _: &str, _: &Cancellation) -> Result<String, Diagnostic> {
            unreachable!()
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn namespaced_selectors_search_by_name_then_match_exactly() {
        let service = DiscoveryService::new(NamespacedGateway, Repository::default());
        let cancel = Cancellation::default();
        for source in [None, Some("beta/skills")] {
            assert_eq!(
                service
                    .preview("author/review", source, &cancel)
                    .await
                    .unwrap()
                    .rendered_text,
                "namespaced preview"
            );
        }
        assert_eq!(
            service
                .preview("other/review", None, &cancel)
                .await
                .unwrap_err()
                .code,
            "skill_not_found"
        );
        assert!(service
            .preview("skills/review/SKILL.md", None, &cancel)
            .await
            .is_ok());
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
    async fn strict_search_is_exact_and_stable_across_upstream_order() {
        let mut original = hits();
        original[0].description = "Provides guided code checks".into();
        original.push(original[0].clone());
        let expected = rank_search("review", original.clone()).unwrap();
        original.reverse();
        assert_eq!(rank_search("review", original.clone()).unwrap(), expected);
        assert_eq!(expected.len(), 3);
        assert_eq!(expected[0].skill.repo, "alpha/skills");
        assert_eq!(expected[1].skill.repo, "beta/skills");
        assert_eq!(expected[0].reason, "exact name match");
        assert_eq!(expected[2].reason, "exact keyword match");
        assert!(!expected
            .iter()
            .any(|item| item.skill.skill_name == "review-helper"));
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

    fn discover_candidate(
        id: &str,
        name: &str,
        description: &str,
        categories: &[&str],
        capabilities: &[&str],
        source_priority: u8,
    ) -> DiscoverCandidate {
        let provenance = ProvenanceEvidence {
            kind: if source_priority == 0 {
                ProvenanceKind::LocalWorkspace
            } else {
                ProvenanceKind::GithubRepository
            },
            confidence: if source_priority == 0 {
                ProvenanceConfidence::Inferred
            } else {
                ProvenanceConfidence::Probable
            },
            raw_evidence: Some(id.to_string()),
            explanation: "fixture evidence".to_string(),
        };
        DiscoverCandidate {
            item: DiscoveredSkill {
                id: id.to_string(),
                rank: 0,
                model_rank: None,
                identity: SkillIdentity {
                    id: id.to_string(),
                    name: name.to_string(),
                    source: "file:///skills".to_string(),
                    relative_path: name.to_string(),
                },
                source: SkillSource {
                    id: format!("source:{id}"),
                    locator: "file:///skills".to_string(),
                },
                description: description.to_string(),
                categories: categories.iter().map(|value| value.to_string()).collect(),
                capabilities: capabilities.iter().map(|value| value.to_string()).collect(),
                provenance: provenance.clone(),
                match_reasons: Vec::new(),
            },
            score: 0,
            source_priority,
            provenance_priority: provenance_priority(provenance.confidence),
        }
    }

    #[test]
    fn discover_ranks_intent_and_metadata_stably_and_collapses_identity_duplicates() {
        let mut candidates = vec![
            discover_candidate(
                "skill:z",
                "deploy-helper",
                "Automates routine service release tasks",
                &["delivery"],
                &["deploy services"],
                1,
            ),
            discover_candidate(
                "skill:a",
                "review-helper",
                "Reviews Rust code and suggests focused changes",
                &["engineering"],
                &["review code"],
                0,
            ),
            discover_candidate(
                "skill:a",
                "review-helper",
                "Reviews Rust code and suggests focused changes",
                &[],
                &[],
                1,
            ),
            discover_candidate(
                "skill:b",
                "code-audit",
                "Checks code changes for security issues",
                &["security"],
                &["audit code"],
                1,
            ),
        ];
        let expected = rank_discover("review rust", candidates.clone());
        candidates.reverse();
        assert_eq!(rank_discover("review rust", candidates), expected);
        assert_eq!(expected.len(), 1);
        assert_eq!(expected[0].id, "skill:a");
        assert_eq!(expected[0].provenance.kind, ProvenanceKind::LocalWorkspace);
        assert!(expected[0]
            .match_reasons
            .iter()
            .any(|reason| reason == "name keyword matches: 1"));

        let duplicate_metadata = vec![
            discover_candidate(
                "skill:duplicate",
                "review-helper",
                "Reviews Rust code",
                &["zeta"],
                &["write tests"],
                0,
            ),
            discover_candidate(
                "skill:duplicate",
                "review-helper",
                "Reviews Rust code",
                &["alpha"],
                &["write docs"],
                0,
            ),
        ];
        let deduplicated = rank_discover("review rust", duplicate_metadata.clone());
        let mut reversed_duplicates = duplicate_metadata;
        reversed_duplicates.reverse();
        assert_eq!(
            rank_discover("review rust", reversed_duplicates),
            deduplicated
        );
        assert_eq!(deduplicated.len(), 1);
        assert_eq!(deduplicated[0].categories, vec!["alpha"]);

        let category = rank_discover(
            "category:security",
            vec![
                discover_candidate(
                    "skill:security",
                    "audit",
                    "Finds security issues",
                    &["security"],
                    &[],
                    1,
                ),
                discover_candidate("skill:other", "review", "Reviews code", &[], &[], 1),
            ],
        );
        assert_eq!(category.len(), 1);
        assert_eq!(category[0].id, "skill:security");
        assert!(category[0]
            .match_reasons
            .iter()
            .any(|reason| reason == "category match: security"));

        let capability = rank_discover(
            "capability:review code",
            vec![discover_candidate(
                "skill:review",
                "review",
                "Reviews code",
                &[],
                &["review code"],
                1,
            )],
        );
        assert_eq!(capability.len(), 1);
        assert!(capability[0]
            .match_reasons
            .iter()
            .any(|reason| reason == "capability match: review code"));

        let tied = rank_discover(
            "review",
            vec![
                discover_candidate("skill:z", "review", "Review code", &[], &[], 1),
                discover_candidate("skill:a", "review", "Review code", &[], &[], 1),
            ],
        );
        assert_eq!(
            tied.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            vec!["skill:a", "skill:z"]
        );
    }

    #[test]
    fn model_reranking_is_explicit_validated_and_keeps_baseline_rank() {
        let mut result = DiscoverResult {
            query: "review".into(),
            items: rank_discover(
                "review",
                vec![
                    discover_candidate("skill:a", "review", "Review code", &[], &[], 1),
                    discover_candidate("skill:b", "review-helper", "Review code", &[], &[], 1),
                ],
            )
            .into_iter()
            .enumerate()
            .map(|(index, mut item)| {
                item.rank = index + 1;
                item
            })
            .collect(),
            eligible_sources: Vec::new(),
            remote_expansion: RemoteExpansionStatus::Complete,
            reranking: RerankingStatus::NotRequested,
            diagnostics: Vec::new(),
        };
        let baseline = result
            .items
            .iter()
            .map(|item| (item.id.clone(), item.rank))
            .collect::<std::collections::BTreeMap<_, _>>();
        apply_model_ranking(&mut result, &["skill:b".to_string(), "skill:a".to_string()]).unwrap();
        assert_eq!(result.reranking, RerankingStatus::Applied);
        assert_eq!(result.items[0].id, "skill:b");
        assert_eq!(result.items[0].model_rank, Some(1));
        assert_eq!(result.items[0].rank, baseline["skill:b"]);
        assert_eq!(
            apply_model_ranking(&mut result, &["skill:a".to_string()])
                .unwrap_err()
                .code,
            "invalid_rerank_output"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn offline_discover_keeps_local_results_and_reports_remote_expansion() {
        let root = std::env::temp_dir().join(format!(
            "tasc-discover-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let skill_path = root.join("review");
        std::fs::create_dir_all(&skill_path).unwrap();
        std::fs::write(
            skill_path.join("SKILL.md"),
            "---\nname: review-helper\ndescription: Reviews code\nmetadata:\n  categories:\n    - engineering\n  capabilities:\n    - review code\n---\n",
        )
        .unwrap();
        let local = LocalSkill {
            identity: SkillIdentity {
                id: "skill:file:///skills/review".into(),
                name: "review-helper".into(),
                source: "file:///skills".into(),
                relative_path: "review".into(),
            },
            target: tasc_core::AdapterTarget {
                id: "target:copilot:project".into(),
                assistant: "copilot".into(),
                path: root.to_string_lossy().into_owned(),
            },
            path: root.join("review").to_string_lossy().into_owned(),
            description: "Reviews code".into(),
            source_revision: None,
            content_digest: "digest".into(),
        };
        let repository = Repository::default();
        repository
            .0
            .borrow_mut()
            .watchlists
            .push(tasc_core::Watchlist {
                id: "watchlist:alpha".into(),
                source: normalize_source("alpha/skills").unwrap(),
                path: None,
                last_observed_revision: None,
                last_check_id: None,
            });
        let service = DiscoveryService::new(Gateway(Vec::new(), RefCell::default()), repository);
        let result = service
            .discover("review code", vec![local], false, &Cancellation::default())
            .await
            .unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].categories, vec!["engineering"]);
        assert_eq!(result.items[0].capabilities, vec!["review code"]);
        assert_eq!(
            result.items[0].provenance.kind,
            ProvenanceKind::LocalWorkspace
        );
        assert_eq!(result.items[0].id, "skill:file:///skills/review");
        assert_eq!(result.eligible_sources.len(), 1);
        assert_eq!(result.remote_expansion, RemoteExpansionStatus::Unavailable);
        assert_eq!(result.diagnostics[0].code, "remote_expansion_unavailable");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unavailable_remote_discovery_preserves_diagnostic_in_result() {
        let service = DiscoveryService::new(UnavailableGateway, Repository::default());
        let result = service
            .discover("review", Vec::new(), true, &Cancellation::default())
            .await
            .unwrap();
        assert_eq!(result.remote_expansion, RemoteExpansionStatus::Unavailable);
        assert_eq!(result.diagnostics[0].code, "gh_authentication");
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
            rank_search("review", invalid).unwrap_err().code,
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
