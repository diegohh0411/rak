use std::sync::OnceLock;

use reqwest::blocking::Client;
use serde_json::{Value, json};

use super::credentials::LeetcodeCredentials;
use super::models::{CheckResult, QuestionDetail, QuestionSummary};

const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
const BASE_URL: &str = "https://leetcode.com";
const GRAPHQL_URL: &str = "https://leetcode.com/graphql";

pub struct LeetcodeClient {
    client: Client,
    creds: Option<LeetcodeCredentials>,
    /// Lazily fetched from the LeetCode homepage when credentials lack a csrf_token.
    csrf_fallback: OnceLock<String>,
}

impl LeetcodeClient {
    pub fn new(creds: Option<LeetcodeCredentials>) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;
        Ok(Self {
            client,
            creds,
            csrf_fallback: OnceLock::new(),
        })
    }

    // -----------------------------------------------------------------------
    // Auth helpers
    // -----------------------------------------------------------------------

    /// CSRF token to use for requests. Prefers the one from credentials;
    /// falls back to lazily fetching one from the LeetCode homepage.
    fn csrf(&self) -> &str {
        if let Some(c) = &self.creds {
            if !c.csrf_token.is_empty() {
                return &c.csrf_token;
            }
        }
        self.csrf_fallback.get_or_init(|| {
            self.client
                .get(format!("{BASE_URL}/"))
                .send()
                .ok()
                .and_then(|r| {
                    r.headers()
                        .get_all("set-cookie")
                        .iter()
                        .find_map(|v| {
                            let s = v.to_str().ok()?;
                            let part = s.split(';').next()?;
                            let (k, val) = part.split_once('=')?;
                            if k.trim() == "csrftoken" {
                                Some(val.trim().to_string())
                            } else {
                                None
                            }
                        })
                })
                .unwrap_or_default()
        })
    }

    fn cookie_header(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(c) = &self.creds {
            if !c.session.is_empty() {
                parts.push(format!("LEETCODE_SESSION={}", c.session));
            }
        }
        let csrf = self.csrf();
        if !csrf.is_empty() {
            parts.push(format!("csrftoken={csrf}"));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        }
    }

    // -----------------------------------------------------------------------
    // HTTP primitives
    // -----------------------------------------------------------------------

    fn post_graphql(&self, body: Value) -> Result<Value, String> {
        let csrf = self.csrf().to_string();
        let mut req = self
            .client
            .post(GRAPHQL_URL)
            .header("Content-Type", "application/json")
            .header("Referer", BASE_URL)
            .header("x-csrftoken", &csrf);

        if let Some(cookie) = self.cookie_header() {
            req = req.header("Cookie", cookie);
        }

        let resp = req
            .json(&body)
            .send()
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("LeetCode API returned HTTP {}", resp.status()));
        }

        resp.json::<Value>()
            .map_err(|e| format!("JSON decode failed: {e}"))
    }

    fn post_json(&self, url: &str, body: Value) -> Result<Value, String> {
        let csrf = self.csrf().to_string();
        let mut req = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Referer", BASE_URL)
            .header("x-csrftoken", &csrf);

        if let Some(cookie) = self.cookie_header() {
            req = req.header("Cookie", cookie);
        }

        let resp = req
            .json(&body)
            .send()
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {} on POST {url}", resp.status()));
        }

        resp.json::<Value>()
            .map_err(|e| format!("JSON decode failed: {e}"))
    }

    fn get_json(&self, url: &str) -> Result<Value, String> {
        let csrf = self.csrf().to_string();
        let mut req = self
            .client
            .get(url)
            .header("Referer", BASE_URL)
            .header("x-csrftoken", &csrf);

        if let Some(cookie) = self.cookie_header() {
            req = req.header("Cookie", cookie);
        }

        let resp = req
            .send()
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {} on GET {url}", resp.status()));
        }

        resp.json::<Value>()
            .map_err(|e| format!("JSON decode failed: {e}"))
    }

    // -----------------------------------------------------------------------
    // Problem list
    // -----------------------------------------------------------------------

    /// Fetch all problems via the REST endpoint — no CSRF or auth required.
    pub fn fetch_all_problems(&self) -> Result<Vec<QuestionSummary>, String> {
        #[derive(serde::Deserialize)]
        struct ApiResponse {
            stat_status_pairs: Vec<StatStatusPair>,
        }
        #[derive(serde::Deserialize)]
        struct StatStatusPair {
            stat: Stat,
            status: Option<String>,
            difficulty: Difficulty,
        }
        #[derive(serde::Deserialize)]
        #[allow(non_snake_case)]
        struct Stat {
            frontend_question_id: u32,
            question__title: String,
            question__title_slug: String,
        }
        #[derive(serde::Deserialize)]
        struct Difficulty {
            level: u8,
        }

        let resp = self
            .client
            .get(format!("{BASE_URL}/api/problems/all/"))
            .header("Referer", BASE_URL)
            .send()
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("LeetCode API returned HTTP {}", resp.status()));
        }

        let data: ApiResponse = resp
            .json()
            .map_err(|e| format!("JSON decode failed: {e}"))?;

        let summaries = data
            .stat_status_pairs
            .into_iter()
            .map(|p| QuestionSummary {
                frontend_id: p.stat.frontend_question_id.to_string(),
                title_slug: p.stat.question__title_slug,
                title: p.stat.question__title,
                difficulty: match p.difficulty.level {
                    1 => "Easy".to_string(),
                    2 => "Medium".to_string(),
                    3 => "Hard".to_string(),
                    _ => "Unknown".to_string(),
                },
                topic_tags: vec![],
                status: p.status,
            })
            .collect();

        Ok(summaries)
    }

    // -----------------------------------------------------------------------
    // Problem detail
    // -----------------------------------------------------------------------

    /// Fetch full detail for a single problem by titleSlug.
    pub fn fetch_question_detail(&self, title_slug: &str) -> Result<QuestionDetail, String> {
        const QUERY: &str = r#"
            query questionData($titleSlug: String!) {
              question(titleSlug: $titleSlug) {
                questionId
                questionFrontendId
                titleSlug
                title
                difficulty
                topicTags { slug name }
                status
                content
                codeSnippets { langSlug code }
              }
            }
        "#;

        let body = json!({
            "query": QUERY,
            "variables": { "titleSlug": title_slug }
        });

        let resp = self.post_graphql(body)?;

        let question = resp
            .pointer("/data/question")
            .ok_or_else(|| format!("problem '{}' not found", title_slug))?;

        if question.is_null() {
            return Err(format!("problem '{}' not found", title_slug));
        }

        serde_json::from_value(question.clone())
            .map_err(|e| format!("failed to parse question detail: {e}"))
    }

    // -----------------------------------------------------------------------
    // Daily challenge
    // -----------------------------------------------------------------------

    /// Fetch today's daily challenge titleSlug.
    pub fn fetch_daily_slug(&self) -> Result<String, String> {
        const QUERY: &str = r#"
            query dailyChallenge {
              activeDailyCodingChallengeQuestion {
                question {
                  titleSlug
                }
              }
            }
        "#;

        let body = json!({ "query": QUERY, "variables": {} });
        let resp = self.post_graphql(body)?;

        resp.pointer("/data/activeDailyCodingChallengeQuestion/question/titleSlug")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "failed to read daily challenge titleSlug".to_string())
    }

    // -----------------------------------------------------------------------
    // Submission
    // -----------------------------------------------------------------------

    /// Submit code for a problem. Returns the submission ID.
    /// Requires credentials — returns an error if none are set.
    pub fn submit(
        &self,
        title_slug: &str,
        question_id: &str,
        lang: &str,
        code: &str,
    ) -> Result<u64, String> {
        if self.creds.is_none() {
            return Err(
                "submission requires authentication — run `rak pull` first to set up credentials"
                    .to_string(),
            );
        }

        let url = format!("{BASE_URL}/problems/{title_slug}/submit/");
        let body = json!({
            "lang": lang,
            "question_id": question_id,
            "questionSlug": title_slug,
            "typed_code": code,
        });

        let resp = self.post_json(&url, body)?;

        resp.get("submission_id")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "submit response missing submission_id".to_string())
    }

    /// Poll `/submissions/detail/{id}/check/` until state == "SUCCESS" or timeout.
    /// Prints dots to stderr while waiting.
    pub fn poll_result(&self, submission_id: u64) -> Result<CheckResult, String> {
        let url = format!("{BASE_URL}/submissions/detail/{submission_id}/check/");
        let max_attempts = 30;

        for attempt in 0..max_attempts {
            if attempt > 0 {
                let delay = if attempt < 5 { 1000 } else { 2000 };
                std::thread::sleep(std::time::Duration::from_millis(delay));
                eprint!(".");
            }

            let resp = self.get_json(&url)?;
            let result: CheckResult = serde_json::from_value(resp)
                .map_err(|e| format!("failed to parse check result: {e}"))?;

            if result.state == "SUCCESS" {
                return Ok(result);
            }
        }

        Err(format!(
            "submission {submission_id} timed out after {max_attempts} polling attempts"
        ))
    }
}
