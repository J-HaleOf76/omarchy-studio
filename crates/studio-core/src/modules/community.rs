//! Community themes directory (ROADMAP 0.9.1): the vendored catalog of the
//! Omarchy manual's extra-themes page, plus everything needed to preview a
//! theme before installing it — the raw `colors.toml` URLs the browser
//! fetches and the install slug `omarchy-theme-install` will produce.
//!
//! The catalog is embedded at build time from `data/community-themes.tsv`
//! (name<TAB>owner/repo); re-scrape with `tools/refresh-themes.sh`.
//! Installing always shells to `omarchy-theme-install`, which clones the
//! repo into the user themes dir **and applies it** — Studio never
//! reimplements that pipeline.

/// One directory entry: a display name and its GitHub `owner/repo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityTheme {
    pub name: String,
    pub repo: String,
}

impl CommunityTheme {
    /// The theme dir name `omarchy-theme-install` derives — mirrors its
    /// shell exactly: basename, drop `.git`, strip a literal `omarchy-`
    /// prefix and `-theme` suffix (before lowercasing, like the script).
    pub fn install_slug(&self) -> String {
        let base = self.repo.rsplit('/').next().unwrap_or(&self.repo);
        let base = base.strip_suffix(".git").unwrap_or(base);
        let base = base.strip_prefix("omarchy-").unwrap_or(base);
        let base = base.strip_suffix("-theme").unwrap_or(base);
        base.to_lowercase()
    }

    /// The URL handed to `omarchy-theme-install`.
    pub fn clone_url(&self) -> String {
        format!("https://github.com/{}.git", self.repo)
    }

    /// Raw `colors.toml` candidates, default-branch guesses in order.
    pub fn colors_urls(&self) -> [String; 2] {
        [
            format!(
                "https://raw.githubusercontent.com/{}/main/colors.toml",
                self.repo
            ),
            format!(
                "https://raw.githubusercontent.com/{}/master/colors.toml",
                self.repo
            ),
        ]
    }
}

/// The embedded directory, in the manual's order. Malformed lines are
/// skipped rather than failing the whole catalog.
pub fn catalog() -> Vec<CommunityTheme> {
    include_str!("../../../../data/community-themes.tsv")
        .lines()
        .filter_map(|line| {
            let (name, repo) = line.split_once('\t')?;
            let (name, repo) = (name.trim(), repo.trim());
            if name.is_empty() || !repo.contains('/') {
                return None;
            }
            Some(CommunityTheme {
                name: name.to_string(),
                repo: repo.to_string(),
            })
        })
        .collect()
}

/// Case-insensitive substring filter over name and repo.
pub fn matches(t: &CommunityTheme, query: &str) -> bool {
    let q = query.to_lowercase();
    t.name.to_lowercase().contains(&q) || t.repo.to_lowercase().contains(&q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_and_is_deduplicated() {
        let all = catalog();
        assert!(all.len() >= 100, "directory holds 100+ themes");
        let mut repos: Vec<_> = all.iter().map(|t| &t.repo).collect();
        repos.sort();
        repos.dedup();
        assert_eq!(repos.len(), all.len(), "no duplicate repos");
        assert!(all.iter().all(|t| t.repo.contains('/')));
    }

    #[test]
    fn install_slug_mirrors_the_install_script() {
        let slug = |repo: &str| {
            CommunityTheme {
                name: String::new(),
                repo: repo.into(),
            }
            .install_slug()
        };
        // prefix + suffix stripped
        assert_eq!(slug("bjarneo/omarchy-ash-theme"), "ash");
        // no prefix/suffix — untouched
        assert_eq!(slug("JJDizz1L/aetheria"), "aetheria");
        // underscore names survive
        assert_eq!(slug("ankur311sudo/black_arch"), "black_arch");
        // bare "omarchy" repo: neither pattern matches
        assert_eq!(slug("eldritch-theme/omarchy"), "omarchy");
        // lowercasing happens after the strips, like the script's sed | tr
        assert_eq!(
            slug("oldjobobo/omarchy-windows-dark-mode-theme"),
            "windows-dark-mode"
        );
    }

    #[test]
    fn urls_point_at_the_repo() {
        let t = CommunityTheme {
            name: "Ash".into(),
            repo: "bjarneo/omarchy-ash-theme".into(),
        };
        assert_eq!(
            t.clone_url(),
            "https://github.com/bjarneo/omarchy-ash-theme.git"
        );
        assert!(t.colors_urls()[0].contains("/main/colors.toml"));
        assert!(t.colors_urls()[1].contains("/master/colors.toml"));
    }

    #[test]
    fn matching_is_case_insensitive_over_name_and_repo() {
        let t = CommunityTheme {
            name: "Rose Pine Dark".into(),
            repo: "guilhermetk/omarchy-rose-pine-dark".into(),
        };
        assert!(matches(&t, "rose"));
        assert!(matches(&t, "PINE"));
        assert!(matches(&t, "guilhermetk"));
        assert!(!matches(&t, "gruvbox"));
    }
}
