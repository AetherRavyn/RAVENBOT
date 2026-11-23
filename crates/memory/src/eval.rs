//! Retrieval-quality evaluation for the configured embedding backend.
//!
//! The benchmark is a fixed set of 50 facts and 10 queries. It is deliberately
//! separate from user memory: running it never reads, writes or sends stored
//! memories. The report tells the user whether the selected backend actually
//! ranks the expected fact in the top results.

use crate::embedding::{cosine_similarity, EmbeddingProvider};
use serde::Serialize;

pub const EVAL_K: usize = 5;

/// Fixed facts. IDs are stable and must stay unique.
pub const EVAL_FACTS: &[(&str, &str)] = &[
    (
        "f01",
        "The release train departs every Thursday at 14:00 UTC.",
    ),
    ("f02", "Invoices are signed with the finance Ed25519 key."),
    ("f03", "Customer backups are retained for thirty days."),
    (
        "f04",
        "The staging database is rebuilt every Monday morning.",
    ),
    ("f05", "API tokens expire after ninety days."),
    (
        "f06",
        "The incident commander posts a summary within one hour of resolution.",
    ),
    (
        "f07",
        "New hires receive laptop and VPN access on their first day.",
    ),
    (
        "f08",
        "The design review happens before implementation starts.",
    ),
    ("f09", "Production deploys require two approvals."),
    ("f10", "The support queue is triaged every two hours."),
    ("f11", "All customer data stays in the Frankfurt region."),
    ("f12", "The on-call rotation changes on Wednesday at noon."),
    (
        "f13",
        "Feature flags are removed after the rollout completes.",
    ),
    (
        "f14",
        "The analytics pipeline runs in fifteen-minute windows.",
    ),
    (
        "f15",
        "Security patches are applied within seventy-two hours.",
    ),
    ("f16", "The documentation site is rebuilt on every merge."),
    ("f17", "Refunds above five hundred dollars need a manager."),
    (
        "f18",
        "The mobile app supports the last three operating-system versions.",
    ),
    (
        "f19",
        "Database migrations run before the application starts.",
    ),
    (
        "f20",
        "The public status page is updated during every outage.",
    ),
    ("f21", "Contractors use a separate identity provider."),
    (
        "f22",
        "The billing export is generated on the first of each month.",
    ),
    ("f23", "Test fixtures never contain production records."),
    ("f24", "The office printer is on the second floor."),
    (
        "f25",
        "Budget alerts fire at ninety percent of the monthly limit.",
    ),
    (
        "f26",
        "The archive bucket uses cold storage after one year.",
    ),
    ("f27", "Internal tools are reachable only through the VPN."),
    (
        "f28",
        "The accessibility audit covers keyboard-only navigation.",
    ),
    ("f29", "Every service exposes a health endpoint."),
    ("f30", "Logs are redacted before they leave the host."),
    (
        "f31",
        "The team retro is held on the last Friday of the quarter.",
    ),
    (
        "f32",
        "API changes are announced two weeks before deprecation.",
    ),
    ("f33", "The backup restore drill happens every six months."),
    ("f34", "Package versions are pinned and reviewed weekly."),
    ("f35", "The sales demo uses synthetic customer data."),
    (
        "f36",
        "Office doors lock automatically at eight in the evening.",
    ),
    ("f37", "Browser sessions are isolated per agent."),
    ("f38", "The certificate renews sixty days before expiry."),
    ("f39", "The data warehouse is read-only for analysts."),
    ("f40", "The escalation path starts with the support lead."),
    (
        "f41",
        "Release notes are written for every user-visible change.",
    ),
    (
        "f42",
        "The finance team closes the books on the fifth business day.",
    ),
    ("f43", "New API endpoints need an OpenAPI schema."),
    (
        "f44",
        "The design system tokens are the only source of colors.",
    ),
    ("f45", "The disaster recovery plan is stored offline."),
    (
        "f46",
        "The vendor review checks data-processing agreements.",
    ),
    ("f47", "The monitoring alert goes to the on-call pager."),
    ("f48", "User research sessions are recorded with consent."),
    (
        "f49",
        "The build cache is cleared when the toolchain changes.",
    ),
    (
        "f50",
        "The quarterly security training is mandatory for everyone.",
    ),
];

/// Fixed queries and the fact each one should retrieve.
pub const EVAL_QUERIES: &[(&str, &str)] = &[
    ("When does the release train leave?", "f01"),
    ("Which cryptographic key signs invoices?", "f02"),
    ("How long do customer backups stay?", "f03"),
    ("When is the staging database rebuilt?", "f04"),
    ("How long do API tokens last?", "f05"),
    (
        "What happens within an hour after an incident is fixed?",
        "f06",
    ),
    ("How many approvals does a production deploy need?", "f09"),
    ("Which region stores customer data?", "f11"),
    ("When does the on-call rotation change?", "f12"),
    ("Who needs to approve a large refund?", "f17"),
];

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalEvalQuery {
    pub query: String,
    pub expected: String,
    pub rank: Option<usize>,
    pub top: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalEvalReport {
    pub k: usize,
    pub dimension: usize,
    pub recall_at_k: f32,
    pub mean_reciprocal_rank: f32,
    pub queries: Vec<RetrievalEvalQuery>,
}

/// Embed the fixed benchmark texts and score the ranking. User memory is never
/// read or sent by this function.
pub async fn evaluate_retrieval(
    provider: &dyn EmbeddingProvider,
) -> Result<RetrievalEvalReport, String> {
    let fact_texts: Vec<&str> = EVAL_FACTS.iter().map(|(_, text)| *text).collect();
    let fact_vectors = provider
        .embed_batch(&fact_texts)
        .await
        .map_err(|error| error.to_string())?;
    let dimension = validate_vectors(&fact_vectors, EVAL_FACTS.len(), "facts")?;

    let query_texts: Vec<&str> = EVAL_QUERIES.iter().map(|(query, _)| *query).collect();
    let query_vectors = provider
        .embed_batch(&query_texts)
        .await
        .map_err(|error| error.to_string())?;
    let query_dimension = validate_vectors(&query_vectors, EVAL_QUERIES.len(), "queries")?;
    if query_dimension != dimension {
        return Err(format!(
            "The embedding provider returned {dimension}-dimension facts and {query_dimension}-dimension queries."
        ));
    }

    let mut results = Vec::with_capacity(EVAL_QUERIES.len());
    let mut hits = 0usize;
    let mut reciprocal_rank_sum = 0.0f32;

    for ((query, expected), query_vector) in EVAL_QUERIES.iter().zip(query_vectors.iter()) {
        let expected_index = EVAL_FACTS
            .iter()
            .position(|(id, _)| id == expected)
            .ok_or_else(|| format!("The benchmark is missing fact {expected}."))?;
        let mut scored: Vec<(usize, f32)> = fact_vectors
            .iter()
            .enumerate()
            .map(|(index, vector)| (index, cosine_similarity(query_vector, vector)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let rank = scored
            .iter()
            .position(|(index, _)| *index == expected_index)
            .map(|position| position + 1);
        if let Some(rank) = rank {
            if rank <= EVAL_K {
                hits += 1;
            }
            reciprocal_rank_sum += 1.0 / rank as f32;
        }
        let top = scored
            .iter()
            .take(EVAL_K)
            .map(|(index, _)| EVAL_FACTS[*index].0.to_string())
            .collect();
        results.push(RetrievalEvalQuery {
            query: (*query).to_string(),
            expected: (*expected).to_string(),
            rank,
            top,
        });
    }

    let total = EVAL_QUERIES.len() as f32;
    Ok(RetrievalEvalReport {
        k: EVAL_K,
        dimension,
        recall_at_k: hits as f32 / total,
        mean_reciprocal_rank: reciprocal_rank_sum / total,
        queries: results,
    })
}

fn validate_vectors(vectors: &[Vec<f32>], expected: usize, label: &str) -> Result<usize, String> {
    if vectors.len() != expected {
        return Err(format!(
            "The embedding provider returned {} {label} vectors instead of {expected}.",
            vectors.len()
        ));
    }
    let dimension = vectors.first().map(Vec::len).unwrap_or(0);
    if dimension == 0 {
        return Err("The embedding provider returned an empty vector.".to_string());
    }
    if vectors
        .iter()
        .any(|vector| vector.len() != dimension || vector.iter().any(|value| !value.is_finite()))
    {
        return Err("The embedding provider returned inconsistent vector sizes.".to_string());
    }
    Ok(dimension)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::EmbeddingError;

    #[test]
    fn fixture_ids_are_unique_and_queries_reference_real_facts() {
        let mut ids = std::collections::HashSet::new();
        for (id, _) in EVAL_FACTS {
            assert!(ids.insert(*id), "duplicate fact id {id}");
        }
        assert_eq!(EVAL_FACTS.len(), 50);
        assert_eq!(EVAL_QUERIES.len(), 10);
        for (_, expected) in EVAL_QUERIES {
            assert!(ids.contains(expected), "missing expected fact {expected}");
        }
    }

    struct FixtureProvider {
        adversarial: bool,
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for FixtureProvider {
        async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
            let dimension = EVAL_FACTS.len();
            if let Some(index) = EVAL_FACTS.iter().position(|(_, fact)| *fact == text) {
                let mut vector = vec![0.0; dimension];
                vector[index] = 1.0;
                return Ok(vector);
            }
            if let Some((_, expected)) = EVAL_QUERIES.iter().find(|(query, _)| *query == text) {
                let index = EVAL_FACTS
                    .iter()
                    .position(|(id, _)| id == expected)
                    .unwrap();
                let mut vector = vec![0.0; dimension];
                vector[index] = if self.adversarial { -1.0 } else { 1.0 };
                return Ok(vector);
            }
            Ok(vec![0.0; dimension])
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
            let mut vectors = Vec::with_capacity(texts.len());
            for text in texts {
                vectors.push(self.embed(text).await?);
            }
            Ok(vectors)
        }

        fn dimension(&self) -> usize {
            EVAL_FACTS.len()
        }
    }

    #[tokio::test]
    async fn a_perfect_provider_scores_full_recall() {
        let report = evaluate_retrieval(&FixtureProvider { adversarial: false })
            .await
            .expect("evaluate");
        assert_eq!(report.k, EVAL_K);
        assert_eq!(report.dimension, 50);
        assert_eq!(report.recall_at_k, 1.0);
        assert_eq!(report.mean_reciprocal_rank, 1.0);
        assert!(report.queries.iter().all(|query| query.rank == Some(1)));
    }

    #[tokio::test]
    async fn an_adversarial_provider_scores_zero_recall() {
        let report = evaluate_retrieval(&FixtureProvider { adversarial: true })
            .await
            .expect("evaluate");
        assert_eq!(report.recall_at_k, 0.0);
        assert!(
            (report.mean_reciprocal_rank - 1.0 / EVAL_FACTS.len() as f32).abs() < 1e-6,
            "MRR counts the reciprocal of the true rank even outside the cutoff"
        );
        assert!(report
            .queries
            .iter()
            .all(|query| query.rank == Some(EVAL_FACTS.len())));
    }
}
