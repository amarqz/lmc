use crate::db::Database;
use crate::index::{IndexApp, IndexEntry};
use anyhow::Result;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct ClusterData {
    pub alias: String,
    pub last_used: Option<i64>,
    pub command_count: usize,
    pub tags: Vec<String>,
    pub commands: Vec<String>,
    pub directory: Option<String>,
}

pub struct SearchResult {
    pub alias: String,
    pub last_used: Option<i64>,
    pub command_count: usize,
    pub tags: Vec<String>,
    pub score: i64,
}

pub fn score_clusters(query: &str, clusters: &[ClusterData], now_secs: i64) -> Vec<SearchResult> {
    let matcher = SkimMatcherV2::default();
    const RECENCY_BONUS_MAX: i64 = 50;
    const WEEK_SECS: i64 = 7 * 24 * 3600;
    // Minimum fuzzy score per query character to reject scattered low-quality matches
    let min_score = query.len() as i64 * 8;

    let mut results: Vec<SearchResult> = clusters
        .iter()
        .filter_map(|c| {
            let searchable = format!(
                "{} {} {} {}",
                c.alias,
                c.commands.join(" "),
                c.tags.join(" "),
                c.directory.as_deref().unwrap_or(""),
            );
            let fuzzy_score = matcher.fuzzy_match(&searchable, query)?;
            if fuzzy_score < min_score {
                return None;
            }
            let recency_bonus = c.last_used.map_or(0, |lu| {
                let age_secs = now_secs.saturating_sub(lu);
                let weeks_old = age_secs / WEEK_SECS;
                RECENCY_BONUS_MAX.saturating_sub(weeks_old * 5)
            });
            Some(SearchResult {
                alias: c.alias.clone(),
                last_used: c.last_used,
                command_count: c.command_count,
                tags: c.tags.clone(),
                score: fuzzy_score + recency_bonus,
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.cmp(&a.score));
    results
}

pub fn run(query: &str, db: &Database) -> Result<()> {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let clusters = db.get_all_clusters()?;
    let aliased: Vec<_> = clusters.into_iter().filter(|c| c.alias.is_some()).collect();

    let mut cluster_data = Vec::new();
    for cluster in &aliased {
        let id = cluster.id.expect("cluster from DB always has id");
        let tags = db.get_tags_for_cluster(id)?;
        let command_count = db.get_command_count_for_cluster(id)?;
        let command_records = db.get_commands_for_cluster(id)?;
        let commands: Vec<String> = command_records.into_iter().map(|r| r.cmd).collect();
        cluster_data.push(ClusterData {
            alias: cluster.alias.clone().unwrap(),
            last_used: cluster.last_used,
            command_count,
            tags,
            commands,
            directory: cluster.directory.clone(),
        });
    }

    let results = score_clusters(query, &cluster_data, now_secs);

    if results.is_empty() {
        eprintln!(
            "No results for \"{}\". Try `lmc` to browse all saved aliases.",
            query
        );
        return Ok(());
    }

    let entries: Vec<IndexEntry> = results
        .into_iter()
        .map(|r| IndexEntry {
            alias: r.alias,
            last_used: r.last_used,
            command_count: r.command_count,
            tags: r.tags,
        })
        .collect();

    let mut app = IndexApp::new(entries);
    if let Some(alias) = crate::index::run_tui(&mut app)? {
        crate::retrieval::run(&alias, db)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn helm_cluster() -> ClusterData {
        ClusterData {
            alias: "helm-debug-prod".to_string(),
            last_used: Some(1_700_000_000),
            command_count: 5,
            tags: vec!["kubernetes".to_string(), "helm".to_string()],
            commands: vec![
                "helm list -n production".to_string(),
                "helm rollback my-app 2".to_string(),
            ],
            directory: Some("/infra".to_string()),
        }
    }

    fn db_cluster() -> ClusterData {
        ClusterData {
            alias: "db-migrate".to_string(),
            last_used: Some(1_699_000_000),
            command_count: 3,
            tags: vec!["postgres".to_string()],
            commands: vec!["psql -U admin mydb".to_string()],
            directory: Some("/backend".to_string()),
        }
    }

    #[test]
    fn test_matching_alias_is_returned() {
        let clusters = vec![helm_cluster(), db_cluster()];
        let results = score_clusters("helm", &clusters, 1_700_100_000);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].alias, "helm-debug-prod");
    }

    #[test]
    fn test_no_match_returns_empty() {
        let clusters = vec![db_cluster()];
        let results = score_clusters("helm", &clusters, 1_700_000_000);
        assert!(results.is_empty());
    }

    #[test]
    fn test_matches_command_string() {
        // "rollback" is in the commands, not the alias
        let clusters = vec![helm_cluster(), db_cluster()];
        let results = score_clusters("rollback", &clusters, 1_700_100_000);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].alias, "helm-debug-prod");
    }

    #[test]
    fn test_matches_tag() {
        let clusters = vec![helm_cluster(), db_cluster()];
        let results = score_clusters("postgres", &clusters, 1_700_100_000);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].alias, "db-migrate");
    }

    #[test]
    fn test_matches_directory() {
        let clusters = vec![helm_cluster(), db_cluster()];
        let results = score_clusters("backend", &clusters, 1_700_100_000);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].alias, "db-migrate");
    }

    #[test]
    fn test_results_sorted_descending_by_score() {
        let clusters = vec![
            ClusterData {
                alias: "git-flow".to_string(),
                last_used: Some(1_699_000_000),
                command_count: 2,
                tags: vec!["git".to_string()],
                commands: vec!["git push origin main".to_string()],
                directory: None,
            },
            ClusterData {
                alias: "helm-debug-prod".to_string(),
                last_used: Some(1_699_000_000),
                command_count: 5,
                tags: vec!["helm".to_string()],
                commands: vec!["helm rollback my-app 2".to_string()],
                directory: None,
            },
        ];
        // "helm rollback" should score higher for the helm cluster
        let results = score_clusters("helm rollback", &clusters, 1_700_000_000);
        assert!(!results.is_empty());
        assert_eq!(results[0].alias, "helm-debug-prod");
    }

    #[test]
    fn test_recency_breaks_tie() {
        // Both match "helm" in alias only; newer one wins via recency bonus
        let clusters = vec![
            ClusterData {
                alias: "helm-old".to_string(),
                last_used: Some(1_690_000_000), // ~16 weeks before now_secs
                command_count: 1,
                tags: vec!["helm".to_string()],
                commands: vec![],
                directory: None,
            },
            ClusterData {
                alias: "helm-new".to_string(),
                last_used: Some(1_700_000_000), // ~0 weeks before now_secs
                command_count: 1,
                tags: vec!["helm".to_string()],
                commands: vec![],
                directory: None,
            },
        ];
        // now_secs = 1_700_100_000: helm-new age ~0 weeks (bonus 50), helm-old age ~16 weeks (bonus 0)
        let results = score_clusters("helm", &clusters, 1_700_100_000);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].alias, "helm-new");
    }
}
