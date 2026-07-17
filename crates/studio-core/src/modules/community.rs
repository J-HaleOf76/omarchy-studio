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

use serde::Deserialize;

/// Wallpaper extensions previewable in-terminal — stills only, matching the
/// Themes screen (a theme's `backgrounds/` may also hold video wallpapers).
const STILL_EXTS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

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

    /// Contents-API listing of the theme's `backgrounds/` directory. Raw
    /// URLs can't list a directory and wallpaper file names are arbitrary,
    /// so the wallpaper preview needs the API — which is rate-limited to 60
    /// requests an hour unauthenticated, hence the on-disk cache above it.
    pub fn backgrounds_url(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/contents/backgrounds",
            self.repo
        )
    }
}

/// A wallpaper file in a theme repo, from the contents API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Background {
    pub name: String,
    pub download_url: String,
}

impl Background {
    /// Lowercased extension, for the cache file name.
    pub fn ext(&self) -> String {
        self.name
            .rsplit_once('.')
            .map(|(_, e)| e.to_lowercase())
            .unwrap_or_else(|| "jpg".to_string())
    }
}

#[derive(Deserialize)]
struct ContentEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    download_url: Option<String>,
}

/// First still wallpaper in a contents listing, by name — the same "first
/// background, sorted" rule the Themes screen previews installed themes
/// with, so a theme looks the same before and after installing. `None` when
/// the listing holds no still image (or isn't a directory listing at all —
/// a missing `backgrounds/` returns a JSON error object, not an array).
pub fn first_background(listing_json: &str) -> Option<Background> {
    let entries: Vec<ContentEntry> = serde_json::from_str(listing_json).ok()?;
    let mut stills: Vec<Background> = entries
        .into_iter()
        .filter(|e| e.kind == "file")
        .filter_map(|e| {
            let ext = e.name.rsplit_once('.')?.1.to_lowercase();
            if !STILL_EXTS.contains(&ext.as_str()) {
                return None;
            }
            Some(Background {
                name: e.name,
                download_url: e.download_url?,
            })
        })
        .collect();
    stills.sort_by(|a, b| a.name.cmp(&b.name));
    stills.into_iter().next()
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
        assert_eq!(
            t.backgrounds_url(),
            "https://api.github.com/repos/bjarneo/omarchy-ash-theme/contents/backgrounds"
        );
    }

    #[test]
    fn first_background_takes_the_first_still_by_name() {
        let listing = r#"[
          {"name": "notes.md", "type": "file", "download_url": "https://x/notes.md"},
          {"name": "nested", "type": "dir", "download_url": null},
          {"name": "2-blue.PNG", "type": "file", "download_url": "https://x/2-blue.PNG"},
          {"name": "clip.mp4", "type": "file", "download_url": "https://x/clip.mp4"},
          {"name": "1-ash.jpg", "type": "file", "download_url": "https://x/1-ash.jpg"}
        ]"#;
        let bg = first_background(listing).expect("a still is listed");
        assert_eq!(bg.name, "1-ash.jpg");
        assert_eq!(bg.download_url, "https://x/1-ash.jpg");
        assert_eq!(bg.ext(), "jpg");

        // Uppercase extensions still count as stills.
        let upper = r#"[{"name": "A.PNG", "type": "file", "download_url": "https://x/A.PNG"}]"#;
        assert_eq!(
            first_background(upper).map(|b| b.ext()).as_deref(),
            Some("png")
        );
    }

    #[test]
    fn a_listing_with_no_still_is_none_not_a_panic() {
        // video-only backgrounds dir
        assert!(first_background(
            r#"[{"name":"a.mp4","type":"file","download_url":"https://x/a.mp4"}]"#
        )
        .is_none());
        // empty dir
        assert!(first_background("[]").is_none());
        // GitHub's 404 body: an object, not an array
        assert!(first_background(r#"{"message":"Not Found"}"#).is_none());
        // not JSON at all
        assert!(first_background("<html>rate limited</html>").is_none());
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
