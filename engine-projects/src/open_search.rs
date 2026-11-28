//! Public catalog discovery. Directory records are data and never become trusted catalog items.

use crate::tools::{Item, plugins};
use reqwest::{Client, Url};
use serde::{Deserialize, de::DeserializeOwned};
use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::Instant;

pub(crate) const SHELF_LIMIT: usize = 8;
const BODY_LIMIT: usize = 512 * 1024;
const QUERY_LIMIT: usize = 256;
const CACHE_LIMIT: usize = 16;
const TTL: Duration = Duration::from_secs(600);
const TIMEOUT: Duration = Duration::from_secs(4);
const SKILLS: &str = "https://skills.sh/api/search";
const SERVERS: &str = "https://registry.modelcontextprotocol.io/v0.1/servers";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub(crate) id: String,
    pub(crate) source: String,
    pub(crate) installs: u64,
    pub(crate) server: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unavailable;

struct Cached {
    query: String,
    expires: Instant,
    hits: Vec<Hit>,
}

static CACHE: Mutex<VecDeque<Cached>> = Mutex::new(VecDeque::new());
static SEARCHING: Semaphore = Semaphore::const_new(2);

/// Only the search words and a fixed result limit go to the directories. The caller grants live access.
pub async fn search(query: &str) -> Result<Vec<Hit>, Unavailable> {
    search_cached(query, &CACHE, async |query| {
        let _slot = SEARCHING.try_acquire().map_err(|_| Unavailable)?;
        crate::tls::install();
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Faber catalog search")
            .build()
            .map_err(|_| Unavailable)?;
        search_with(query, crate::tools::catalog(), |url| fetch(&client, url)).await
    })
    .await
}

async fn search_cached(
    query: &str,
    cache: &Mutex<VecDeque<Cached>>,
    load: impl AsyncFnOnce(&str) -> Result<(Vec<Hit>, bool), Unavailable>,
) -> Result<Vec<Hit>, Unavailable> {
    let query = query.split_whitespace().filter_map(plain_word).take(8).collect::<Vec<_>>().join(" ");
    if query.is_empty() {
        return Ok(Vec::new());
    }
    if query.len() > QUERY_LIMIT {
        return Err(Unavailable);
    }
    {
        let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        cache.retain(|entry| entry.expires > now);
        if let Some(entry) = cache.iter().find(|entry| entry.query == query) {
            return Ok(entry.hits.clone());
        }
    }
    let (hits, complete) = load(&query).await?;
    // A partial answer (one directory down or slow) is kept for a minute, so a struggling directory is not asked again by
    // every search.
    let keep = if complete { TTL } else { PARTIAL_TTL };
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    // A search running at the same time may have cached a complete answer; a partial one never replaces it.
    if !complete && let Some(entry) = cache.iter().find(|entry| entry.query == query && entry.expires > now) {
        return Ok(entry.hits.clone());
    }
    cache.retain(|entry| entry.query != query);
    if cache.len() >= CACHE_LIMIT {
        cache.pop_front();
    }
    cache.push_back(Cached { query, expires: Instant::now() + keep, hits: hits.clone() });
    Ok(hits)
}

const PARTIAL_TTL: Duration = Duration::from_secs(60);

/// One search word as the directories may see it: trimmed of sentence punctuation, keeping inner `+`, `#`, `-` and `_`
/// and a `.js` suffix (Node.js, C++, C#). Paths, dotfiles, domains, file names, e-mails, numbers that look like a PIN,
/// phone, account or document number, and token-shaped words (long, or mixing letters, digits and separators) never
/// leave the PC. Words a person may consider private but that look like plain words cannot be told apart; the tool
/// description asks the AI to send public job words only.
fn plain_word(word: &str) -> Option<&str> {
    let word = word
        .trim_matches(|c: char| matches!(c, ',' | ';' | ':' | '!' | '?' | '"' | '\'' | '(' | ')' | '[' | ']' | '“' | '”' | '‘' | '’'))
        .trim_end_matches('.');
    let digits = word.chars().filter(|c| c.is_numeric()).count();
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    let separators = word.chars().filter(|c| matches!(c, '-' | '_')).count();
    (!word.is_empty()
        && word.chars().count() <= 20
        && !word.starts_with(['.', '-', '_', '+', '#'])
        && word.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '+' | '#' | '-' | '_'))
        && !word.contains("..")
        // A dot only before `js` (Node.js, Vue.js), never a domain or file name.
        && word.rsplit_once('.').is_none_or(|(_, suffix)| suffix.eq_ignore_ascii_case("js"))
        && digits < 4
        && !(letters > 0 && digits > 0 && separators > 0)
        && !(word.chars().count() >= 16 && digits > 0))
        .then_some(word)
}

async fn fetch(client: &Client, url: Url) -> Result<Vec<u8>, Unavailable> {
    let response = client.get(url.clone()).send().await.map_err(|_| Unavailable)?;
    if !response.status().is_success() {
        return Err(Unavailable);
    }
    crate::install::metadata_body(response, BODY_LIMIT, url.as_str()).await.map_err(|_| Unavailable)
}

#[derive(Deserialize)]
struct Skills {
    skills: Vec<Skill>,
}

#[derive(Deserialize)]
struct Skill {
    id: String,
    source: String,
    installs: u64,
}

#[derive(Deserialize)]
struct Servers {
    servers: Vec<ServerRow>,
}

#[derive(Deserialize)]
struct ServerRow {
    server: Server,
}

#[derive(Deserialize)]
struct Server {
    name: String,
    #[serde(default)]
    installs: u64,
    repository: Option<Repository>,
    #[serde(default)]
    packages: Vec<Package>,
}

#[derive(Deserialize)]
struct Repository {
    url: String,
}

#[derive(Deserialize)]
struct Package {
    #[serde(rename = "registryType")]
    registry_type: String,
    identifier: String,
}

fn identifier(raw: &str, parts: usize) -> bool {
    raw.len() <= 192
        && raw.split('/').count() == parts
        && raw.split('/').all(|part| {
            part.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
}

fn github_repo(raw: &str) -> Option<&str> {
    let repo = raw.strip_prefix("https://github.com/")?.trim_end_matches('/');
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    identifier(repo, 2).then_some(repo)
}

fn normalized_name(name: &str) -> String {
    name.chars().filter(|&c| !matches!(c, '-' | '_') && !c.is_whitespace()).flat_map(char::to_lowercase).collect()
}

async fn directory<T: DeserializeOwned>(response: impl Future<Output = Result<Vec<u8>, Unavailable>>) -> Option<T> {
    let bytes = tokio::time::timeout(TIMEOUT, response).await.ok()?.ok()?;
    serde_json::from_slice(&bytes).ok()
}

async fn search_with<F, Fut>(query: &str, catalog: &[Item], fetch: F) -> Result<(Vec<Hit>, bool), Unavailable>
where
    F: Fn(Url) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, Unavailable>>,
{
    let mut skills = Url::parse(SKILLS).expect("skills directory URL");
    skills.query_pairs_mut().append_pair("q", query).append_pair("limit", "16");
    let mut servers = Url::parse(SERVERS).expect("server registry URL");
    servers.query_pairs_mut().append_pair("search", query).append_pair("limit", &SHELF_LIMIT.to_string());
    let (skills, servers) = tokio::join!(directory::<Skills>(fetch(skills)), directory::<Servers>(fetch(servers)));
    if skills.is_none() && servers.is_none() {
        return Err(Unavailable);
    }
    let complete = skills.is_some() && servers.is_some();
    let checked = || catalog.iter().chain(plugins::resources());
    let words = crate::toolbox::english();
    let checked_names: HashSet<_> = checked()
        .flat_map(|item| {
            [
                Some(item.id.as_str()),
                item.cli_name.as_deref(),
                item.path.rsplit('/').next(),
                words.get(format!("catalog.name.{}", item.id).as_str()).map(String::as_str),
            ]
            .into_iter()
            .flatten()
            .map(normalized_name)
        })
        .collect();
    let mut hits = Vec::new();
    for skill in skills.into_iter().flat_map(|rows| rows.skills) {
        if !identifier(&skill.id, 3) || !identifier(&skill.source, 2) {
            continue;
        }
        let Some((repo, name)) = skill.id.rsplit_once('/') else { continue };
        if !repo.eq_ignore_ascii_case(&skill.source) || checked_names.contains(&normalized_name(name)) {
            continue;
        }
        hits.push(Hit { id: skill.id, source: skill.source, installs: skill.installs, server: false });
    }
    for ServerRow { server } in servers.into_iter().flat_map(|rows| rows.servers) {
        if !identifier(&server.name, 2) {
            continue;
        }
        let repo = server.repository.as_ref().and_then(|r| github_repo(&r.url));
        let name = server.name.rsplit('/').next().unwrap_or(&server.name);
        if checked_names.contains(&normalized_name(name))
            || checked().any(|item| {
                let Some(pinned) = &item.server else { return false };
                server.packages.iter().any(|package| package.registry_type == "npm" && package.identifier == pinned.package)
                    || repo.is_some_and(|repo| {
                        item.repo.eq_ignore_ascii_case(repo) && repo.rsplit('/').next().is_some_and(|slug| slug.eq_ignore_ascii_case(name))
                    })
            })
        {
            continue;
        }
        hits.push(Hit { id: server.name, source: repo.unwrap_or_default().into(), installs: server.installs, server: true });
    }
    hits.sort_by(|a, b| b.installs.cmp(&a.installs).then_with(|| a.id.cmp(&b.id)));
    let mut seen = HashSet::new();
    hits.retain(|hit| seen.insert((hit.server, hit.id.to_ascii_lowercase())));
    hits.truncate(SHELF_LIMIT);
    Ok((hits, complete))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolbox::{Entry, catalog};

    const SKILL_ROWS: &str = r#"{"skills":[
        {"id":"obra/superpowers/systematic-debugging","name":"systematic-debugging","installs":99,"source":"obra/superpowers"},
        {"id":"evil/fork/SysTeMaTic_DeBugging","installs":99,"source":"evil/fork"},
        {"id":"person/kit/debug-helper","name":"debug-helper","installs":3,"source":"person/kit"},
        {"id":"another/repo/debugging","name":"debugging","installs":10,"source":"another/repo"},
        {"id":"Another/Repo/Debugging","name":"Debugging","installs":2,"source":"Another/Repo"},
        {"id":"bad/repo/fake\nmine","name":"fake","installs":99,"source":"bad/repo"}
    ]}"#;
    const SERVER_ROWS: &str = r#"{"servers":[
        {"server":{"name":"io.github.browser/debugger","repository":{"url":"https://github.com/browser/debugger"},"version":"1.0.0"}},
        {"server":{"name":"io.github.microsoft/browser-automation","packages":[{"registryType":"npm","identifier":"@playwright/mcp"}],"version":"1.0.0"}},
        {"server":{"name":"io.evil/Play-Wright"}},
        {"server":{"name":"io.evil/CHROME_DevTools"}},
        {"server":{"name":"io.github.chromedevtools/chrome-devtools-mcp","repository":{"url":"https://github.com/ChromeDevTools/chrome-devtools-mcp"},"remotes":[{"type":"streamable-http","url":"https://example.org/mcp"}],"version":"1.0.0"}}
    ],"metadata":{"count":5}}"#;

    fn entries() -> Vec<Entry<'static>> {
        crate::tools::catalog()
            .iter()
            .map(|item| {
                let installed = item.id == "systematic-debugging";
                Entry { item, plugin: false, installed, on: installed }
            })
            .collect()
    }

    #[tokio::test]
    async fn recorded_directories_rank_after_the_kit_and_mine_and_drop_checked_duplicates() {
        let hits = search_with("debugging test", crate::tools::catalog(), |url| async move {
            if url.host_str() == Some("skills.sh") {
                assert_eq!(url.path(), "/api/search");
                assert_eq!(url.query(), Some("q=debugging+test&limit=16"));
                Ok(SKILL_ROWS.as_bytes().to_vec())
            } else {
                assert_eq!(url.host_str(), Some("registry.modelcontextprotocol.io"));
                assert_eq!(url.path(), "/v0.1/servers");
                assert_eq!(url.query(), Some("search=debugging+test&limit=8"));
                Ok(SERVER_ROWS.as_bytes().to_vec())
            }
        })
        .await
        .map(|(hits, _)| hits);
        let local = [("debugger-local".into(), "server")];
        let answer = catalog(&entries(), &local, "debugging a bug", hits);
        let (checked, after) = answer.split_once("\nmine\n").unwrap();
        assert!(checked.starts_with("Faber Kit\nsystematic-debugging — "), "{answer}");
        let (mine, open) = after.split_once("\nopen world\n").unwrap();
        assert_eq!(mine, "systematic-debugging (skill, on)\n\"debugger-local\" (Tool server)");
        let wording = "Unchecked. Faber cannot install this yet; it is listed so you know it exists.";
        let lines = open.lines().map(|line| line.strip_suffix(wording).unwrap()).collect::<Vec<_>>();
        assert_eq!(
            lines,
            [
                "another/repo/debugging (Skill) — ",
                "person/kit/debug-helper (Skill) — ",
                "io.github.browser/debugger (Tool server, github.com/browser/debugger) — "
            ]
        );
    }

    #[tokio::test]
    async fn public_requests_use_at_most_eight_plain_words_and_skip_empty_queries() {
        let cache = Mutex::new(VecDeque::new());
        let private = r"C:\work D:secret a/b folder\file me@example.com https://example.org example.org ref123456 １２３４５６";
        let query = format!("{private} debugging e2e web-design mcp_server a b c d ninth");
        search_cached(&query, &cache, async |query| {
            search_with(query, crate::tools::catalog(), |url| async move {
                let (key, rows) = if url.host_str() == Some("skills.sh") { ("q", SKILL_ROWS) } else { ("search", SERVER_ROWS) };
                let sent = url.query_pairs().find(|(name, _)| name.as_ref() == key).unwrap().1;
                assert_eq!(sent, "debugging e2e web-design mcp_server a b c d");
                Ok(rows.as_bytes().to_vec())
            })
            .await
        })
        .await
        .unwrap();
        assert!(search_cached(private, &cache, async |_| panic!("empty query sent")).await.unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn a_partial_answer_never_replaces_a_complete_one_cached_meanwhile() {
        let cache = Mutex::new(VecDeque::new());
        let hit = |id: &str| Hit { id: id.into(), source: String::new(), installs: 0, server: false };
        let ids = |hits: Vec<Hit>| hits.into_iter().map(|hit| hit.id).collect::<Vec<_>>();
        let partial = search_cached("debugging", &cache, async |query| {
            let complete = search_cached(query, &cache, async |_| Ok((vec![hit("a/b/full"), hit("c/d/full")], true))).await;
            assert_eq!(ids(complete.unwrap()), ["a/b/full", "c/d/full"]);
            Ok((vec![hit("a/b/full")], false))
        })
        .await;
        assert_eq!(ids(partial.unwrap()), ["a/b/full", "c/d/full"]);
        let later = search_cached("debugging", &cache, async |_| panic!("the complete answer was not kept")).await;
        assert_eq!(ids(later.unwrap()), ["a/b/full", "c/d/full"]);
    }

    #[tokio::test(start_paused = true)]
    async fn failed_directories_keep_local_and_other_results_and_are_retried() {
        let cache = Mutex::new(VecDeque::new());
        let local = [("debugger-local".into(), "server")];
        for (skill_rows, server_rows, expected) in [
            (Some(SKILL_ROWS), None, Some("another/repo/debugging")),
            (Some("malformed"), Some(SERVER_ROWS), Some("io.github.browser/debugger")),
            (None, None, None),
            (Some(SKILL_ROWS), Some(SERVER_ROWS), Some("another/repo/debugging")),
        ] {
            let hits = search_cached("debugging", &cache, async |query| {
                search_with(query, crate::tools::catalog(), |url| async move {
                    match (url.host_str() == Some("skills.sh"), skill_rows, server_rows) {
                        (true, Some(rows), _) | (false, _, Some(rows)) => Ok(rows.as_bytes().to_vec()),
                        (true, None, _) => Err(Unavailable),
                        (false, _, None) => std::future::pending().await,
                    }
                })
                .await
            })
            .await;
            let answer = catalog(&entries(), &local, "debugging a bug", hits);
            assert!(answer.starts_with("Faber Kit\n"));
            assert!(answer.contains("\nmine\nsystematic-debugging (skill, on)\n\"debugger-local\" (Tool server)"), "{answer}");
            if let Some(id) = expected {
                assert!(answer.split_once("\nopen world\n").unwrap().1.contains(id), "{answer}");
            } else {
                assert!(answer.ends_with("Public directories are unavailable."), "{answer}");
            }
            // A partial answer is kept only briefly, then both directories are asked again.
            tokio::time::advance(PARTIAL_TTL).await;
        }
        assert!(search_cached("debugging", &cache, async |_| panic!("complete results were not cached")).await.is_ok());
    }
}

#[cfg(test)]
mod query_words {
    use super::plain_word;

    #[test]
    fn only_plain_search_words_leave_the_pc() {
        let kept: Vec<_> = "debugging, Node.js C++ \"pdf\" UI/UX react-hooks".split_whitespace().filter_map(plain_word).collect();
        assert_eq!(kept, ["debugging", "Node.js", "C++", "pdf", "react-hooks"]);
        for private in [
            "example.org",
            "internal.io",
            "payroll.db",
            "notes.txt",
            "/Payroll",
            ".env",
            "４２７１",
            "api_token_supersecret",
            "１２３４５６",
            "123-45-6789",
            "555-1234",
            "sk_live_abc123",
            "ghp_aB3cD4eF5gH6",
            r"C:\Users\me",
            "me@mail.com",
            "https://x.io",
            "a1b2c3d4e5f6g7h8",
        ] {
            assert_eq!(plain_word(private), None, "{private}");
        }
    }
}
