use crate::{
    evaluation::RunArtifactRepository,
    service::{DiscoveredSkill, SkillAdapter, StateRepository},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tasc_core::{
    consistency::{
        ConsistencyArtifact, ConsistencyCategory, ConsistencyCategoryResult, ConsistencyEvidence,
        ConsistencyFinding, ConsistencySkillSnapshot, ConsistencyStatus, EvidenceConfidence,
        EvidenceLayer, FindingSeverity, RevisionStatus,
    },
    discovery::Diagnostic,
    evaluation::RunArtifact,
    LocalState, SourceRevision, LOCAL_STATE_VERSION,
};

static CHECK_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const RULE_VERSION: &str = "1";

pub trait ConsistencyArtifactRepository {
    fn save(&self, artifact: &ConsistencyArtifact) -> Result<(), String>;
    fn list(&self) -> Result<Vec<ConsistencyArtifact>, String>;
    fn get(&self, id: &str) -> Result<ConsistencyArtifact, String>;
}

#[derive(Debug, Clone)]
pub struct FsConsistencyArtifactRepository {
    directory: PathBuf,
}

impl FsConsistencyArtifactRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl ConsistencyArtifactRepository for FsConsistencyArtifactRepository {
    fn save(&self, artifact: &ConsistencyArtifact) -> Result<(), String> {
        fs::create_dir_all(&self.directory)
            .map_err(|error| format!("cannot create Consistency Check directory: {error}"))?;
        let path = artifact_path(&self.directory, &artifact.id)?;
        let contents = serde_json::to_vec_pretty(artifact)
            .map_err(|error| format!("cannot serialize Consistency Check: {error}"))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    "Consistency Check Artifacts are immutable; this artifact ID already exists"
                        .to_string()
                } else {
                    format!("cannot create Consistency Check Artifact: {error}")
                }
            })?;
        if let Err(error) = file.write_all(&contents).and_then(|_| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(path);
            return Err(format!("cannot write Consistency Check Artifact: {error}"));
        }
        Ok(())
    }

    fn list(&self) -> Result<Vec<ConsistencyArtifact>, String> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("cannot list Consistency Check Artifacts: {error}")),
        };
        let mut artifacts = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| format!("cannot read artifact entry: {error}"))?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot inspect Consistency Check Artifact: {error}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(
                    "Consistency Check Artifacts must be regular, non-symlink files".to_string(),
                );
            }
            let artifact: ConsistencyArtifact = serde_json::from_slice(
                &fs::read(&path)
                    .map_err(|error| format!("cannot read Consistency Check Artifact: {error}"))?,
            )
            .map_err(|error| format!("invalid Consistency Check Artifact: {error}"))?;
            if artifact_path(&self.directory, &artifact.id)? != path {
                return Err("Consistency Check Artifact ID does not match its filename".to_string());
            }
            artifacts.push(artifact);
        }
        artifacts.sort_by(|left, right| {
            left.created_at_unix_ms
                .cmp(&right.created_at_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(artifacts)
    }

    fn get(&self, id: &str) -> Result<ConsistencyArtifact, String> {
        let path = artifact_path(&self.directory, id)?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!("no Consistency Check Artifact has ID '{id}'")
            } else {
                format!("cannot inspect Consistency Check Artifact: {error}")
            }
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(
                "Consistency Check Artifacts must be regular, non-symlink files".to_string(),
            );
        }
        let artifact: ConsistencyArtifact = serde_json::from_slice(
            &fs::read(path)
                .map_err(|error| format!("cannot read Consistency Check Artifact: {error}"))?,
        )
        .map_err(|error| format!("invalid Consistency Check Artifact: {error}"))?;
        if artifact.id != id {
            return Err("Consistency Check Artifact ID does not match its filename".to_string());
        }
        Ok(artifact)
    }
}

impl<
        D: SkillAdapter,
        R: StateRepository,
        A: ConsistencyArtifactRepository,
        E: RunArtifactRepository,
    > ConsistencyService<D, R, A, E>
{
    pub fn new(
        adapter: D,
        state_repository: R,
        artifact_repository: A,
        evaluation_repository: E,
    ) -> Self {
        Self {
            adapter,
            state_repository,
            artifact_repository,
            evaluation_repository,
        }
    }

    pub fn check(
        &self,
        selectors: &[String],
        all_installed: bool,
    ) -> Result<ConsistencyArtifact, Diagnostic> {
        if (all_installed && !selectors.is_empty()) || (!all_installed && selectors.is_empty()) {
            return Err(selection_error(
                "invalid_scope",
                "Provide explicit skill selectors or use --all-installed, but not both.",
            ));
        }
        if !all_installed && selectors.len() < 2 {
            return Err(selection_error(
                "insufficient_scope",
                "A Consistency Check requires at least two selected skills; add peers or use --all-installed.",
            ));
        }

        let state = self.state_repository.load().map_err(state_error)?;
        if state.version != LOCAL_STATE_VERSION {
            return Err(state_error(format!(
                "unsupported Local State version {}; expected {}",
                state.version, LOCAL_STATE_VERSION
            )));
        }
        let discovered = self.adapter.discover().map_err(state_error)?;
        let selected = select_skills(&state, &discovered, selectors, all_installed)?;
        if selected.is_empty() {
            return Err(selection_error(
                "empty_scope",
                "No installed skills are available for this Consistency Check.",
            ));
        }
        let mut docs = selected
            .iter()
            .map(read_skill)
            .collect::<Result<Vec<_>, _>>()
            .map_err(state_error)?;
        docs.sort_by(|left, right| {
            left.snapshot
                .identity
                .id
                .cmp(&right.snapshot.identity.id)
                .then_with(|| left.snapshot.path.cmp(&right.snapshot.path))
        });
        let glossary = read_project_glossary();
        let prior_checks = self.artifact_repository.list().map_err(state_error)?;
        let evaluation_artifacts = self.evaluation_repository.list().map_err(state_error)?;
        let stale_artifacts = stale_evidence(&docs, &evaluation_artifacts, &prior_checks);
        let (findings, categories) = analyze(&docs, &glossary, &stale_artifacts);
        let created_at_unix_ms = now_unix_ms();
        let sequence = CHECK_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let artifact = ConsistencyArtifact {
            schema_version: 1,
            id: format!(
                "check-{created_at_unix_ms}-{}-{sequence}",
                std::process::id()
            ),
            created_at_unix_ms,
            completed_at_unix_ms: now_unix_ms(),
            selectors: selectors.to_vec(),
            all_installed,
            analyzer: "tasc-static-consistency".to_string(),
            analyzer_version: env!("CARGO_PKG_VERSION").to_string(),
            rule_version: RULE_VERSION.to_string(),
            skills: docs.into_iter().map(|doc| doc.snapshot).collect(),
            categories,
            findings,
            inferred_evidence_status: ConsistencyStatus::NotRun,
        };
        self.artifact_repository
            .save(&artifact)
            .map_err(state_error)?;
        Ok(artifact)
    }

    pub fn list(&self) -> Result<Vec<ConsistencyArtifact>, Diagnostic> {
        self.artifact_repository.list().map_err(state_error)
    }

    pub fn show(&self, id: &str) -> Result<ConsistencyArtifact, Diagnostic> {
        self.artifact_repository.get(id).map_err(state_error)
    }
}

pub struct ConsistencyService<D, R, A, E> {
    adapter: D,
    state_repository: R,
    artifact_repository: A,
    evaluation_repository: E,
}

#[derive(Debug, Clone)]
struct SkillDoc {
    snapshot: ConsistencySkillSnapshot,
    text: String,
    body: String,
    fields: HashMap<String, Vec<(String, usize)>>,
}

fn select_skills(
    state: &LocalState,
    discovered: &[DiscoveredSkill],
    selectors: &[String],
    all_installed: bool,
) -> Result<Vec<DiscoveredSkill>, Diagnostic> {
    if all_installed {
        return Ok(discovered.to_vec());
    }
    let mut selected = Vec::new();
    let mut seen_selectors = BTreeSet::new();
    for selector in selectors {
        if !seen_selectors.insert(selector) {
            return Err(selection_error(
                "duplicate_selector",
                &format!("Skill selector '{selector}' was provided more than once."),
            ));
        }
        let matches = discovered
            .iter()
            .filter(|skill| {
                skill.identity.id == *selector
                    || skill.identity.name == *selector
                    || format!("installation:{}@{}", skill.identity.id, skill.target.id)
                        == *selector
                    || state.installations.iter().any(|installation| {
                        installation.id == *selector
                            && installation.skill_id == skill.identity.id
                            && installation.target_id == skill.target.id
                    })
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [skill] => {
                if selected.iter().any(|item: &DiscoveredSkill| {
                    item.identity.id == skill.identity.id && item.path == skill.path
                }) {
                    return Err(selection_error(
                        "duplicate_skill",
                        "Selectors resolve to the same installed skill more than once.",
                    ));
                }
                selected.push((*skill).clone());
            }
            [] => {
                return Err(selection_error(
                    "selection_not_found",
                    &format!("No installed skill matches selector '{selector}'."),
                ))
            }
            many => {
                return Err(selection_error(
                    "ambiguous_selection",
                    &format!(
                        "Selector '{selector}' matches multiple installations: {}",
                        many.iter()
                            .map(|skill| format!("{}@{}", skill.identity.id, skill.target.id))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ))
            }
        }
    }
    Ok(selected)
}

fn read_skill(skill: &DiscoveredSkill) -> Result<SkillDoc, String> {
    let path = Path::new(&skill.path);
    let skill_file = path.join("SKILL.md");
    let metadata = fs::symlink_metadata(&skill_file)
        .map_err(|error| format!("cannot inspect '{}': {error}", skill_file.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "selected SKILL.md '{}' must be a regular, non-symlink file",
            skill_file.display()
        ));
    }
    if metadata.len() > 1_048_576 {
        return Err(format!(
            "selected SKILL.md '{}' exceeds the 1 MiB analysis limit",
            skill_file.display()
        ));
    }
    let text = fs::read_to_string(&skill_file)
        .map_err(|error| format!("cannot read '{}': {error}", skill_file.display()))?;
    let (fields, body) = parse_frontmatter(&text);
    Ok(SkillDoc {
        snapshot: ConsistencySkillSnapshot {
            identity: skill.identity.clone(),
            source_revision: skill.source_revision.as_ref().map(|value| SourceRevision {
                kind: "observed".to_string(),
                value: value.clone(),
            }),
            content_digest: skill.content_digest.clone(),
            path: skill.path.clone(),
        },
        text,
        body,
        fields,
    })
}

fn stale_evidence(
    docs: &[SkillDoc],
    artifacts: &[RunArtifact],
    prior_checks: &[ConsistencyArtifact],
) -> Vec<ConsistencyEvidence> {
    let mut stale = Vec::new();
    for doc in docs {
        for artifact in artifacts
            .iter()
            .filter(|artifact| artifact.identity.id == doc.snapshot.identity.id)
        {
            let digest_changed = artifact
                .package_digest
                .as_deref()
                .is_some_and(|digest| digest != doc.snapshot.content_digest);
            let revision_changed = match (
                artifact.source_revision.as_ref(),
                doc.snapshot.source_revision.as_ref(),
            ) {
                (Some(previous), Some(current)) => previous.value != current.value,
                _ => false,
            };
            if digest_changed || revision_changed {
                stale.push(ConsistencyEvidence {
                    layer: EvidenceLayer::Deterministic,
                    kind: "stale_evaluation_artifact".to_string(),
                    skill_id: doc.snapshot.identity.id.clone(),
                    path: format!("evaluations/{}.json", artifact.id),
                    line: None,
                    excerpt: format!(
                        "prior digest={}; current digest={}",
                        artifact.package_digest.as_deref().unwrap_or("unavailable"),
                        doc.snapshot.content_digest
                    ),
                });
            }
            for previous in prior_checks {
                for snapshot in previous
                    .skills
                    .iter()
                    .filter(|snapshot| snapshot.identity.id == doc.snapshot.identity.id)
                {
                    let digest_changed = snapshot.content_digest != doc.snapshot.content_digest;
                    let revision_changed = match (
                        snapshot.source_revision.as_ref(),
                        doc.snapshot.source_revision.as_ref(),
                    ) {
                        (Some(previous), Some(current)) => previous.value != current.value,
                        _ => false,
                    };
                    if digest_changed || revision_changed {
                        stale.push(ConsistencyEvidence {
                            layer: EvidenceLayer::Deterministic,
                            kind: "stale_consistency_artifact".to_string(),
                            skill_id: doc.snapshot.identity.id.clone(),
                            path: format!("consistency-checks/{}.json", previous.id),
                            line: None,
                            excerpt: format!(
                                "prior digest={}; current digest={}",
                                snapshot.content_digest, doc.snapshot.content_digest
                            ),
                        });
                    }
                }
            }
        }
    }
    stale
}

fn parse_frontmatter(text: &str) -> (HashMap<String, Vec<(String, usize)>>, String) {
    let mut lines = text.lines().enumerate();
    if lines.next().is_none_or(|(_, line)| line.trim() != "---") {
        return (HashMap::new(), text.to_string());
    }
    let mut fields = HashMap::new();
    let mut current = None::<String>;
    let mut body_start = text.len();
    for (index, line) in lines {
        if line.trim() == "---" {
            body_start = text
                .lines()
                .take(index + 1)
                .map(|line| line.len() + 1)
                .sum::<usize>()
                .min(text.len());
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("- ") {
            if let Some(key) = &current {
                fields.entry(key.clone()).or_insert_with(Vec::new).push((
                    value.trim().trim_matches(['\'', '"']).to_string(),
                    index + 1,
                ));
            }
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim().to_ascii_lowercase().replace('-', "_");
            if line.starts_with(char::is_whitespace) && current.as_deref() == Some("metadata") {
                current = Some(key.clone());
            } else if !line.starts_with(char::is_whitespace) {
                current = Some(key.clone());
            }
            let value = value.trim();
            if !value.is_empty() {
                for value in split_values(value) {
                    fields
                        .entry(key.clone())
                        .or_insert_with(Vec::new)
                        .push((value, index + 1));
                }
            }
        }
    }
    let body = text.get(body_start..).unwrap_or_default().to_string();
    (fields, body)
}

fn split_values(value: &str) -> Vec<String> {
    let value = value.trim().trim_start_matches('[').trim_end_matches(']');
    value
        .split(',')
        .map(|value| value.trim().trim_matches(['\'', '"']).to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn analyze(
    docs: &[SkillDoc],
    glossary: &[(String, String)],
    stale_evidence: &[ConsistencyEvidence],
) -> (Vec<ConsistencyFinding>, Vec<ConsistencyCategoryResult>) {
    let mut findings = Vec::new();
    let mut statuses = BTreeMap::new();
    check_dependencies(docs, &mut findings, &mut statuses);
    check_ownership(docs, &mut findings, &mut statuses);
    check_triggers(docs, &mut findings, &mut statuses);
    check_assumptions(docs, &mut findings, &mut statuses);
    check_terminology(docs, glossary, &mut findings, &mut statuses);
    check_scope(docs, &mut findings, &mut statuses);
    for evidence in stale_evidence {
        let skill_ids = vec![evidence.skill_id.clone()];
        let mut stale_finding = finding(
            ConsistencyCategory::StaleArtifact,
            "Prior Check or Evaluation evidence references a different package revision.",
            &skill_ids,
            vec![evidence.clone()],
            FindingSeverity::Warning,
            EvidenceConfidence::Observed,
            95,
            "Rerun the Evaluation against the selected package revision before relying on its result.",
        );
        stale_finding.revision_status = RevisionStatus::Stale;
        findings.push(stale_finding);
    }
    statuses.insert(
        category_key(ConsistencyCategory::StaleArtifact),
        category_result(
            ConsistencyCategory::StaleArtifact,
            if stale_evidence.is_empty() {
                ConsistencyStatus::NotFound
            } else {
                ConsistencyStatus::Found
            },
            if stale_evidence.is_empty() {
                "No prior Evaluation Artifact for the selected skills references a different local content digest."
            } else {
                "Prior Evaluation Artifacts reference older local content digests."
            },
        ),
    );
    let categories = all_categories()
        .into_iter()
        .map(|category| {
            statuses.remove(&category_key(category)).unwrap_or_else(|| {
                category_result(category, ConsistencyStatus::NotRun, "Analyzer did not run.")
            })
        })
        .collect();
    findings.sort_by(|left, right| left.id.cmp(&right.id));
    (findings, categories)
}

fn check_dependencies(
    docs: &[SkillDoc],
    findings: &mut Vec<ConsistencyFinding>,
    statuses: &mut BTreeMap<String, ConsistencyCategoryResult>,
) {
    let mut edges = BTreeMap::<String, Vec<(String, &SkillDoc, usize)>>::new();
    let by_name = docs
        .iter()
        .flat_map(|doc| {
            [
                (doc.snapshot.identity.id.to_ascii_lowercase(), doc),
                (doc.snapshot.identity.name.to_ascii_lowercase(), doc),
            ]
        })
        .collect::<HashMap<_, _>>();
    let complete = docs
        .iter()
        .any(|doc| has_any_field(doc, &["required_skills", "dependencies"]));
    for doc in docs {
        for (dependency, line) in field_values(doc, &["required_skills", "dependencies"]) {
            let target = by_name.get(&dependency.to_ascii_lowercase());
            if let Some(target) = target {
                edges
                    .entry(doc.snapshot.identity.id.clone())
                    .or_default()
                    .push((target.snapshot.identity.id.clone(), doc, line));
            } else {
                let evidence = evidence(
                    doc,
                    "declared_dependency",
                    Some(line),
                    &format!("required skill: {dependency}"),
                );
                findings.push(finding(
                    ConsistencyCategory::MissingDependency,
                    &format!("Declared required skill '{dependency}' is not in the selected set."),
                    &[doc.snapshot.identity.id.clone()],
                    vec![evidence],
                    FindingSeverity::Blocking,
                    EvidenceConfidence::Declared,
                    100,
                    "Include the required skill in the selected set or correct the dependency declaration.",
                ));
            }
        }
    }
    let cycles = dependency_cycles(&edges);
    for cycle in cycles {
        let involved = cycle.iter().cloned().collect::<BTreeSet<_>>();
        let mut evidence_items = Vec::new();
        for from in &cycle {
            if let Some((_, doc, line)) = edges
                .get(from)
                .into_iter()
                .flatten()
                .find(|(to, _, _)| involved.contains(to))
            {
                evidence_items.push(evidence(
                    doc,
                    "declared_dependency",
                    Some(*line),
                    &format!(
                        "dependency cycle edge: {from} -> {}",
                        cycle[(cycle
                            .iter()
                            .position(|item| item == from)
                            .unwrap_or_default()
                            + 1)
                            % cycle.len()]
                    ),
                ));
            }
        }
        findings.push(finding(
            ConsistencyCategory::MissingDependency,
            &format!("Declared dependency cycle: {}.", cycle.join(" -> ")),
            &cycle,
            evidence_items,
            FindingSeverity::Warning,
            EvidenceConfidence::Declared,
            95,
            "Break the cycle or document a workflow that resolves it without cyclic ordering.",
        ));
    }
    let has_findings = findings
        .iter()
        .any(|item| item.category == ConsistencyCategory::MissingDependency);
    statuses.insert(
        category_key(ConsistencyCategory::MissingDependency),
        category_result(
            ConsistencyCategory::MissingDependency,
            if has_findings {
                ConsistencyStatus::Found
            } else if complete {
                ConsistencyStatus::NotFound
            } else {
                ConsistencyStatus::Inconclusive
            },
            if complete {
                "Declared dependency metadata was checked against exactly the selected set."
            } else {
                "Dependency metadata is incomplete or could not be parsed."
            },
        ),
    );
}

fn check_ownership(
    docs: &[SkillDoc],
    findings: &mut Vec<ConsistencyFinding>,
    statuses: &mut BTreeMap<String, ConsistencyCategoryResult>,
) {
    let mut owners = BTreeMap::<String, Vec<(&SkillDoc, usize, String)>>::new();
    let declared = docs.iter().any(|doc| {
        has_any_field(
            doc,
            &["ownership", "ownership_claims", "owns", "capabilities"],
        )
    });
    for doc in docs {
        for (claim, line) in field_values(
            doc,
            &["ownership", "ownership_claims", "owns", "capabilities"],
        ) {
            owners
                .entry(normalize(&claim))
                .or_default()
                .push((doc, line, claim));
        }
    }
    for claims in owners.values().filter(|claims| claims.len() > 1) {
        let skills = claims
            .iter()
            .map(|(doc, _, _)| doc.snapshot.identity.id.clone())
            .collect::<Vec<_>>();
        let evidence: Vec<ConsistencyEvidence> = claims
            .iter()
            .map(|(doc, line, claim)| evidence(doc, "ownership_claim", Some(*line), claim))
            .collect();
        let claim = &claims[0].2;
        findings.push(finding(
            ConsistencyCategory::OwnershipOverlap,
            &format!("Multiple selected skills declare the same ownership claim '{claim}'."),
            &skills,
            evidence.clone(),
            FindingSeverity::Warning,
            EvidenceConfidence::Declared,
            90,
            "Review the overlapping claims and narrow their scopes or add an explicit scoped Precedence Rule.",
        ));
        let has_precedence = claims.iter().all(|(doc, _, _)| {
            has_any_field(doc, &["precedence", "precedence_rule", "precedence_rules"])
        });
        if !has_precedence {
            findings.push(finding(
                ConsistencyCategory::PrecedenceAmbiguity,
                &format!("Overlapping ownership claim '{claim}' has no declared Precedence Rule."),
                &skills,
                evidence,
                FindingSeverity::Warning,
                EvidenceConfidence::Declared,
                85,
                "Declare a scoped Precedence Rule or remove the duplicate ownership claim.",
            ));
        }
    }
    let overlaps = findings
        .iter()
        .any(|item| item.category == ConsistencyCategory::OwnershipOverlap);
    statuses.insert(
        category_key(ConsistencyCategory::OwnershipOverlap),
        category_result(
            ConsistencyCategory::OwnershipOverlap,
            if overlaps {
                ConsistencyStatus::Found
            } else if declared {
                ConsistencyStatus::NotFound
            } else {
                ConsistencyStatus::Inconclusive
            },
            if declared {
                "Explicit ownership declarations were compared; free-text claims are not elevated to definitive conflicts."
            } else {
                "No structured Ownership Claims were available; free-text claims are not sufficient to establish a conflict."
            },
        ),
    );
    let ambiguous = findings
        .iter()
        .any(|item| item.category == ConsistencyCategory::PrecedenceAmbiguity);
    statuses.insert(
        category_key(ConsistencyCategory::PrecedenceAmbiguity),
        category_result(
            ConsistencyCategory::PrecedenceAmbiguity,
            if ambiguous {
                ConsistencyStatus::Found
            } else if declared {
                ConsistencyStatus::NotFound
            } else {
                ConsistencyStatus::Inconclusive
            },
            "Only overlapping declared claims are checked for missing or ambiguous precedence.",
        ),
    );
}

fn check_triggers(
    docs: &[SkillDoc],
    findings: &mut Vec<ConsistencyFinding>,
    statuses: &mut BTreeMap<String, ConsistencyCategoryResult>,
) {
    let mut triggers = BTreeMap::<String, Vec<(&SkillDoc, usize, String)>>::new();
    let mut trigger_data = 0;
    for doc in docs {
        for (trigger, line) in field_values(doc, &["triggers", "trigger", "should_trigger"]) {
            trigger_data += 1;
            triggers
                .entry(normalize(&trigger))
                .or_default()
                .push((doc, line, trigger));
        }
    }
    for overlap in triggers.values().filter(|matches| {
        matches
            .iter()
            .map(|(doc, _, _)| &doc.snapshot.identity.id)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    }) {
        let skills = overlap
            .iter()
            .map(|(doc, _, _)| doc.snapshot.identity.id.clone())
            .collect::<Vec<_>>();
        let evidence = overlap
            .iter()
            .map(|(doc, line, trigger)| evidence(doc, "declared_trigger", Some(*line), trigger))
            .collect();
        findings.push(finding(
            ConsistencyCategory::TriggerOverlap,
            &format!(
                "Selected skills declare the same trigger phrase '{}'; lexical overlap alone does not establish runtime collision.",
                overlap[0].2
            ),
            &skills,
            evidence,
            FindingSeverity::Warning,
            EvidenceConfidence::Declared,
            70,
            "Compare the same authored prompts across these skills before changing routing behavior.",
        ));
    }
    let has_overlap = findings
        .iter()
        .any(|item| item.category == ConsistencyCategory::TriggerOverlap);
    statuses.insert(
        category_key(ConsistencyCategory::TriggerOverlap),
        category_result(
            ConsistencyCategory::TriggerOverlap,
            if has_overlap {
                ConsistencyStatus::Found
            } else if trigger_data > 0 {
                ConsistencyStatus::NotFound
            } else {
                ConsistencyStatus::Inconclusive
            },
            if trigger_data > 0 {
                "Declared trigger phrases were compared exactly after case and whitespace normalization; this is not a runtime probe."
            } else {
                "No structured trigger declarations were available for comparison."
            },
        ),
    );

    let have_task_data = docs.iter().any(|doc| {
        has_any_field(
            doc,
            &["required_tasks", "required_capabilities", "capabilities"],
        )
    });
    let mut gap_count = 0;
    for doc in docs {
        let declared_triggers = field_values(doc, &["triggers", "trigger", "should_trigger"]);
        for (task, line) in field_values(
            doc,
            &["required_tasks", "required_capabilities", "capabilities"],
        ) {
            if !declared_triggers
                .iter()
                .any(|(trigger, _)| normalize(trigger).contains(&normalize(&task)))
            {
                gap_count += 1;
                findings.push(finding(
                    ConsistencyCategory::TriggerGap,
                    &format!(
                        "Declared capability or required task '{task}' has no matching positive trigger declaration."
                    ),
                    &[doc.snapshot.identity.id.clone()],
                    vec![evidence(
                        doc,
                        "declared_required_surface",
                        Some(line),
                        &task,
                    )],
                    FindingSeverity::Warning,
                    EvidenceConfidence::Declared,
                    75,
                    "Add an authored trigger example or confirm that this surface is intentionally not user-triggered.",
                ));
            }
        }
    }
    statuses.insert(
        category_key(ConsistencyCategory::TriggerGap),
        category_result(
            ConsistencyCategory::TriggerGap,
            if gap_count > 0 {
                ConsistencyStatus::Found
            } else if have_task_data {
                ConsistencyStatus::NotFound
            } else {
                ConsistencyStatus::Inconclusive
            },
            if have_task_data {
                "Only explicitly declared required tasks or capabilities are tested for exact trigger coverage."
            } else {
                "No declared required task surface was available; generic absence of trigger phrases is not treated as a gap."
            },
        ),
    );
}

fn check_assumptions(
    docs: &[SkillDoc],
    findings: &mut Vec<ConsistencyFinding>,
    statuses: &mut BTreeMap<String, ConsistencyCategoryResult>,
) {
    let mut declarations = BTreeMap::<String, Vec<(&SkillDoc, usize, String)>>::new();
    let mut has_metadata = false;
    for doc in docs {
        for (assumption, line) in field_values(doc, &["assumptions", "requires", "forbids"]) {
            has_metadata = true;
            let normalized = normalize(&assumption);
            let (claim, polarity) = if let Some(value) = normalized.strip_prefix("not ") {
                (value.to_string(), false)
            } else if let Some(value) = normalized.strip_prefix("requires ") {
                (value.to_string(), true)
            } else if let Some(value) = normalized.strip_prefix("forbids ") {
                (value.to_string(), false)
            } else {
                continue;
            };
            declarations.entry(claim).or_default().push((
                doc,
                line,
                if polarity { "positive" } else { "negative" }.to_string(),
            ));
        }
    }
    for (claim, declarations) in declarations {
        if !declarations
            .iter()
            .any(|(_, _, polarity)| polarity == "positive")
            || !declarations
                .iter()
                .any(|(_, _, polarity)| polarity == "negative")
        {
            continue;
        }
        let skills = declarations
            .iter()
            .map(|(doc, _, _)| doc.snapshot.identity.id.clone())
            .collect::<Vec<_>>();
        let evidence = declarations
            .iter()
            .map(|(doc, line, polarity)| {
                evidence(
                    doc,
                    "declared_assumption",
                    Some(*line),
                    &format!("{polarity}: {claim}"),
                )
            })
            .collect();
        findings.push(finding(
            ConsistencyCategory::IncompatibleAssumption,
            &format!("Selected skills make incompatible declarations about '{claim}'."),
            &skills,
            evidence,
            FindingSeverity::Warning,
            EvidenceConfidence::Declared,
            90,
            "Review the conflicting declarations and align their prerequisites or exclusions.",
        ));
    }
    let found = findings
        .iter()
        .any(|item| item.category == ConsistencyCategory::IncompatibleAssumption);
    statuses.insert(
        category_key(ConsistencyCategory::IncompatibleAssumption),
        category_result(
            ConsistencyCategory::IncompatibleAssumption,
            if found {
                ConsistencyStatus::Found
            } else if has_metadata {
                ConsistencyStatus::NotFound
            } else {
                ConsistencyStatus::Inconclusive
            },
            if has_metadata {
                "Explicit positive and negative prerequisite declarations were compared."
            } else {
                "No structured assumption or prerequisite declarations were available."
            },
        ),
    );
}

fn check_terminology(
    docs: &[SkillDoc],
    glossary: &[(String, String)],
    findings: &mut Vec<ConsistencyFinding>,
    statuses: &mut BTreeMap<String, ConsistencyCategoryResult>,
) {
    let mut found = false;
    for (term, glossary_excerpt) in glossary {
        for doc in docs {
            if contains_term(&doc.text, term) {
                found = true;
                findings.push(finding(
                    ConsistencyCategory::TerminologyDrift,
                    &format!(
                        "Skill '{}' uses glossary-avoided term '{term}'.",
                        doc.snapshot.identity.name
                    ),
                    &[doc.snapshot.identity.id.clone()],
                    vec![
                        evidence(doc, "glossary_term", find_term_line(&doc.text, term), term),
                        ConsistencyEvidence {
                            layer: EvidenceLayer::Deterministic,
                            kind: "project_glossary".to_string(),
                            skill_id: doc.snapshot.identity.id.clone(),
                            path: "CONTEXT.md".to_string(),
                            line: None,
                            excerpt: glossary_excerpt.clone(),
                        },
                    ],
                    FindingSeverity::Warning,
                    EvidenceConfidence::Declared,
                    85,
                    &format!("Use the project glossary's preferred term instead of '{term}'."),
                ));
            }
        }
    }
    let status = if found {
        ConsistencyStatus::Found
    } else if glossary.is_empty() {
        ConsistencyStatus::Inconclusive
    } else {
        ConsistencyStatus::NotFound
    };
    statuses.insert(
        category_key(ConsistencyCategory::TerminologyDrift),
        category_result(
            ConsistencyCategory::TerminologyDrift,
            status,
            if glossary.is_empty() {
                "No project glossary was available; no model-assisted terminology discovery was run."
            } else {
                "Skill text was checked for whole-word use of project-glossary avoided terms."
            },
        ),
    );
}

fn check_scope(
    docs: &[SkillDoc],
    findings: &mut Vec<ConsistencyFinding>,
    statuses: &mut BTreeMap<String, ConsistencyCategoryResult>,
) {
    let mut declared = false;
    let mut found = false;
    for doc in docs {
        for (non_goal, line) in field_values(doc, &["non_goals", "non_goal"]) {
            declared = true;
            let phrase = normalize(&non_goal);
            for (body_line, content) in doc.body.lines().enumerate() {
                let text = normalize(content);
                if text.contains(&phrase)
                    && ![
                        "do not",
                        "does not",
                        "never",
                        "must not",
                        "should not",
                        "avoid",
                    ]
                    .iter()
                    .any(|negation| text.contains(negation))
                {
                    found = true;
                    findings.push(finding(
                        ConsistencyCategory::ScopeLeak,
                        &format!(
                            "Instruction appears to pursue declared non-goal '{non_goal}'."
                        ),
                        &[doc.snapshot.identity.id.clone()],
                        vec![
                            evidence(
                                doc,
                                "declared_non_goal",
                                Some(line),
                                &non_goal,
                            ),
                            evidence(
                                doc,
                                "instruction",
                                Some(body_line + 1),
                                content.trim(),
                            ),
                        ],
                        FindingSeverity::Warning,
                        EvidenceConfidence::Observed,
                        70,
                        "Review whether the instruction violates the declared non-goal or the boundary needs clarification.",
                    ));
                }
            }
        }
    }
    statuses.insert(
        category_key(ConsistencyCategory::ScopeLeak),
        category_result(
            ConsistencyCategory::ScopeLeak,
            if found {
                ConsistencyStatus::Found
            } else if declared {
                ConsistencyStatus::NotFound
            } else {
                ConsistencyStatus::Inconclusive
            },
            if declared {
                "Only explicit non-goals and matching positive instruction text are compared."
            } else {
                "No explicit non-goals were available; unstructured scope inference was not run."
            },
        ),
    );
}

fn read_project_glossary() -> Vec<(String, String)> {
    let Ok(context) = fs::read_to_string("CONTEXT.md") else {
        return Vec::new();
    };
    let mut current_term = String::new();
    let mut terms = Vec::new();
    for line in context.lines() {
        if let Some(term) = line
            .trim()
            .strip_prefix("**")
            .and_then(|value| value.strip_suffix("**:"))
        {
            current_term = term.to_string();
        } else if let Some(avoids) = line.trim().strip_prefix("_Avoid_:") {
            for avoided in avoids
                .split(',')
                .map(str::trim)
                .filter(|term| !term.is_empty())
            {
                terms.push((
                    avoided.trim_matches(['*', '`']).to_string(),
                    format!("{} — Avoid: {avoided}", current_term),
                ));
            }
        }
    }
    terms
}

fn field_values(doc: &SkillDoc, fields: &[&str]) -> Vec<(String, usize)> {
    fields
        .iter()
        .filter_map(|field| doc.fields.get(*field))
        .flatten()
        .cloned()
        .filter(|(value, _)| !value.trim().is_empty())
        .collect()
}

fn has_any_field(doc: &SkillDoc, fields: &[&str]) -> bool {
    fields.iter().any(|field| {
        doc.fields
            .get(*field)
            .is_some_and(|values| !values.is_empty())
    })
}

fn dependency_cycles(
    edges: &BTreeMap<String, Vec<(String, &SkillDoc, usize)>>,
) -> Vec<Vec<String>> {
    fn visit(
        node: &str,
        edges: &BTreeMap<String, Vec<(String, &SkillDoc, usize)>>,
        visiting: &mut Vec<String>,
        visited: &mut BTreeSet<String>,
        cycles: &mut BTreeSet<Vec<String>>,
    ) {
        if let Some(position) = visiting.iter().position(|item| item == node) {
            let mut cycle = visiting[position..].to_vec();
            if let Some((smallest, _)) = cycle.iter().enumerate().min_by_key(|(_, id)| *id) {
                cycle.rotate_left(smallest);
            }
            cycles.insert(cycle);
            return;
        }
        if !visited.insert(node.to_string()) {
            return;
        }
        visiting.push(node.to_string());
        for (dependency, _, _) in edges.get(node).into_iter().flatten() {
            visit(dependency, edges, visiting, visited, cycles);
        }
        visiting.pop();
    }
    let mut visited = BTreeSet::new();
    let mut cycles = BTreeSet::new();
    for node in edges.keys() {
        visit(node, edges, &mut Vec::new(), &mut visited, &mut cycles);
    }
    cycles.into_iter().collect()
}

fn finding(
    category: ConsistencyCategory,
    claim: &str,
    skill_ids: &[String],
    evidence: Vec<ConsistencyEvidence>,
    severity: FindingSeverity,
    confidence: EvidenceConfidence,
    confidence_score: u8,
    recommendation: &str,
) -> ConsistencyFinding {
    let mut skill_ids = skill_ids.to_vec();
    skill_ids.sort();
    skill_ids.dedup();
    let key = format!(
        "{category:?}|{}|{RULE_VERSION}|{}",
        skill_ids.join("|"),
        normalize(claim)
    );
    ConsistencyFinding {
        id: format!(
            "finding:{}:{:016x}",
            category_key(category),
            stable_hash(key.as_bytes())
        ),
        category,
        status: ConsistencyStatus::Found,
        revision_status: RevisionStatus::Current,
        severity,
        confidence,
        confidence_score,
        claim: claim.to_string(),
        skill_ids,
        evidence,
        recommendation: recommendation.to_string(),
    }
}

fn evidence(doc: &SkillDoc, kind: &str, line: Option<usize>, excerpt: &str) -> ConsistencyEvidence {
    ConsistencyEvidence {
        layer: EvidenceLayer::Deterministic,
        kind: kind.to_string(),
        skill_id: doc.snapshot.identity.id.clone(),
        path: format!("{}/SKILL.md", doc.snapshot.path),
        line,
        excerpt: excerpt.chars().take(240).collect(),
    }
}

fn category_result(
    category: ConsistencyCategory,
    status: ConsistencyStatus,
    reason: &str,
) -> ConsistencyCategoryResult {
    ConsistencyCategoryResult {
        category,
        status,
        reason: reason.to_string(),
    }
}

fn all_categories() -> [ConsistencyCategory; 9] {
    [
        ConsistencyCategory::TriggerOverlap,
        ConsistencyCategory::TriggerGap,
        ConsistencyCategory::OwnershipOverlap,
        ConsistencyCategory::MissingDependency,
        ConsistencyCategory::IncompatibleAssumption,
        ConsistencyCategory::PrecedenceAmbiguity,
        ConsistencyCategory::TerminologyDrift,
        ConsistencyCategory::StaleArtifact,
        ConsistencyCategory::ScopeLeak,
    ]
}

fn category_key(category: ConsistencyCategory) -> String {
    format!("{category:?}")
        .chars()
        .flat_map(|character| {
            if character.is_ascii_uppercase() {
                vec!['_', character.to_ascii_lowercase()]
            } else {
                vec![character]
            }
        })
        .collect::<String>()
        .trim_start_matches('_')
        .to_string()
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn contains_term(text: &str, term: &str) -> bool {
    let text = lexical_normalize(text);
    let term = lexical_normalize(term);
    format!(" {text} ").contains(&format!(" {term} "))
}

fn lexical_normalize(text: &str) -> String {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn find_term_line(text: &str, term: &str) -> Option<usize> {
    text.lines()
        .position(|line| contains_term(line, term))
        .map(|line| line + 1)
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn artifact_path(directory: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
    {
        return Err("invalid Consistency Check Artifact ID".to_string());
    }
    Ok(directory.join(format!("{id}.json")))
}

fn selection_error(code: &str, message: &str) -> Diagnostic {
    Diagnostic::new(
        code,
        "usage",
        "consistency",
        message,
        "Select two or more unambiguous installed skills, or explicitly use --all-installed.",
    )
}

fn state_error(message: String) -> Diagnostic {
    Diagnostic::new(
        "consistency_failed",
        "operational",
        "consistency",
        &message,
        "Inspect Local State, selected skill packages, and Consistency Check artifact storage, then retry.",
    )
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasc_core::{
        consistency::ConsistencyCategory,
        evaluation::{
            EvaluationAnalyzer, EvaluationDiagnostic, EvaluationOutcome, EvaluationPolicy,
        },
        AdapterTarget, SkillIdentity,
    };

    fn doc(id: &str, name: &str, text: &str) -> SkillDoc {
        let (fields, body) = parse_frontmatter(text);
        SkillDoc {
            snapshot: ConsistencySkillSnapshot {
                identity: SkillIdentity {
                    id: id.to_string(),
                    name: name.to_string(),
                    source: "fixture".to_string(),
                    relative_path: name.to_string(),
                },
                source_revision: Some(SourceRevision::commit("current".to_string())),
                content_digest: format!("digest-{id}"),
                path: format!("/skills/{name}"),
            },
            text: text.to_string(),
            body,
            fields,
        }
    }

    #[test]
    fn reports_declared_dependencies_cycles_and_independent_categories() {
        let docs = vec![
            doc(
                "alpha",
                "alpha",
                "---\nname: alpha\nrequired_skills:\n  - beta\ntriggers:\n  - run review\nrequired_tasks:\n  - audit permissions\nownership:\n  - workflow files\nassumptions:\n  - requires isolated runner\nnon_goals:\n  - deployment\n---\nUse the plugin to run review. Perform deployment tasks.\n",
            ),
            doc(
                "beta",
                "beta",
                "---\nname: beta\nrequired_skills:\n  - alpha\ntriggers:\n  - run review\nownership:\n  - workflow files\nassumptions:\n  - forbids isolated runner\nnon_goals:\n  - deployment\n---\nDo not perform deployment tasks.\n",
            ),
        ];
        let (findings, categories) = analyze(
            &docs,
            &[("plugin".to_string(), "Avoid: plugin".to_string())],
            &[],
        );
        let has = |category| findings.iter().any(|finding| finding.category == category);

        assert!(has(ConsistencyCategory::MissingDependency));
        assert!(has(ConsistencyCategory::OwnershipOverlap));
        assert!(has(ConsistencyCategory::PrecedenceAmbiguity));
        assert!(has(ConsistencyCategory::TriggerOverlap));
        assert!(has(ConsistencyCategory::TriggerGap));
        assert!(has(ConsistencyCategory::IncompatibleAssumption));
        assert!(has(ConsistencyCategory::TerminologyDrift));
        assert!(has(ConsistencyCategory::ScopeLeak));
        assert_eq!(categories.len(), 9);
        assert!(categories.iter().any(|result| {
            result.category == ConsistencyCategory::TriggerOverlap
                && result.status == ConsistencyStatus::Found
        }));
        assert!(findings.iter().all(|finding| {
            finding
                .evidence
                .iter()
                .all(|evidence| evidence.layer == EvidenceLayer::Deterministic)
        }));
        assert!(findings.iter().any(|finding| {
            finding.claim.contains("dependency cycle")
                && finding.severity == FindingSeverity::Warning
        }));
    }

    #[test]
    fn avoids_inferring_conflicts_from_unstructured_or_negated_text() {
        let docs = vec![
            doc(
                "alpha",
                "alpha",
                "---\nname: alpha\ntriggers:\n  - review pull request\nnon_goals:\n  - deployment\nassumptions:\n  - requires isolated runner\n---\nDo not perform deployment tasks.\n",
            ),
            doc(
                "beta",
                "beta",
                "---\nname: beta\ntriggers:\n  - review code changes\nassumptions:\n  - requires isolated runner\n---\n",
            ),
        ];
        let (findings, categories) = analyze(&docs, &[], &[]);
        assert!(!findings.iter().any(|finding| {
            matches!(
                finding.category,
                ConsistencyCategory::ScopeLeak
                    | ConsistencyCategory::TriggerOverlap
                    | ConsistencyCategory::IncompatibleAssumption
                    | ConsistencyCategory::OwnershipOverlap
            )
        }));
        assert!(categories.iter().any(|result| {
            result.category == ConsistencyCategory::ScopeLeak
                && result.status == ConsistencyStatus::NotFound
        }));
        assert!(categories.iter().any(|result| {
            result.category == ConsistencyCategory::OwnershipOverlap
                && result.status == ConsistencyStatus::Inconclusive
        }));
    }

    #[test]
    fn reports_unselected_declared_dependencies_and_stale_evaluation_evidence() {
        let docs = vec![doc(
            "alpha",
            "alpha",
            "---\nname: alpha\nrequired_skills:\n  - not-selected\n---\n",
        )];
        let artifact = RunArtifact {
            schema_version: 1,
            id: "run-old".to_string(),
            created_at_unix_ms: 1,
            completed_at_unix_ms: 2,
            selector: "alpha".to_string(),
            identity: docs[0].snapshot.identity.clone(),
            source_revision: Some(SourceRevision::commit("old".to_string())),
            target: Some(AdapterTarget {
                id: "target".to_string(),
                assistant: "copilot".to_string(),
                path: "/target".to_string(),
            }),
            installation_id: None,
            package_path: Some("/skills/alpha".to_string()),
            package_digest: Some("old-digest".to_string()),
            analyzer: EvaluationAnalyzer {
                name: "smoke".to_string(),
                version: "1".to_string(),
                configuration: Vec::new(),
            },
            policy: EvaluationPolicy {
                mode: "package_only".to_string(),
                package_validation: "required".to_string(),
                scenario_execution: "not_run".to_string(),
            },
            outcome: EvaluationOutcome::Passed,
            package_outcome: EvaluationOutcome::Passed,
            execution_outcome: EvaluationOutcome::NotRun,
            diagnostics: Vec::<EvaluationDiagnostic>::new(),
        };
        let stale = stale_evidence(&docs, &[artifact], &[]);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].layer, EvidenceLayer::Deterministic);
        let (findings, categories) = analyze(&docs, &[], &stale);
        assert!(findings.iter().any(|finding| {
            finding.category == ConsistencyCategory::StaleArtifact
                && finding.claim.contains("different package revision")
                && finding.revision_status == RevisionStatus::Stale
        }));
        assert!(findings
            .iter()
            .any(|finding| finding.category == ConsistencyCategory::MissingDependency));
        assert!(categories.iter().any(|result| {
            result.category == ConsistencyCategory::StaleArtifact
                && result.status == ConsistencyStatus::Found
        }));
    }

    #[test]
    fn logical_finding_ids_survive_evidence_revision_changes() {
        let first = doc(
            "alpha",
            "alpha",
            "---\nname: alpha\ntriggers:\n  - same trigger\n---\n",
        );
        let second = doc(
            "beta",
            "beta",
            "---\nname: beta\ntriggers:\n  - same trigger\n---\n",
        );
        let mut changed_evidence = second.clone();
        changed_evidence.snapshot.content_digest = "new-digest".to_string();
        let first_result = analyze(&[first.clone(), second.clone()], &[], &[]).0;
        let second_result = analyze(&[first, changed_evidence], &[], &[]).0;
        let first_id = first_result
            .iter()
            .find(|finding| finding.category == ConsistencyCategory::TriggerOverlap)
            .unwrap()
            .id
            .clone();
        let second_id = second_result
            .iter()
            .find(|finding| finding.category == ConsistencyCategory::TriggerOverlap)
            .unwrap()
            .id
            .clone();
        assert_eq!(first_id, second_id);
    }

    #[test]
    fn filesystem_check_artifacts_are_immutable_and_queryable() {
        let directory = std::env::temp_dir().join(format!(
            "tasc-consistency-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        let doc = doc("alpha", "alpha", "---\nname: alpha\n---\n");
        let artifact = ConsistencyArtifact {
            schema_version: 1,
            id: "check-test".to_string(),
            created_at_unix_ms: 1,
            completed_at_unix_ms: 2,
            selectors: vec!["alpha".to_string()],
            all_installed: true,
            analyzer: "test".to_string(),
            analyzer_version: "1".to_string(),
            rule_version: "1".to_string(),
            skills: vec![doc.snapshot],
            categories: Vec::new(),
            findings: Vec::new(),
            inferred_evidence_status: ConsistencyStatus::NotRun,
        };
        let repository = FsConsistencyArtifactRepository::new(directory.clone());
        repository.save(&artifact).unwrap();
        assert!(repository.save(&artifact).is_err());
        assert_eq!(repository.list().unwrap(), vec![artifact.clone()]);
        assert_eq!(repository.get("check-test").unwrap(), artifact);
        fs::remove_dir_all(directory).unwrap();
    }
}
