use reqwest::blocking::Client;
use serde_json::{Value, json};

use super::models::{QuestionDetail, QuestionSummary};

const GRAPHQL_URL: &str = "https://leetcode.com/graphql";

pub struct LeetcodeClient {
    client: Client,
    session: Option<String>,
}

impl LeetcodeClient {
    pub fn new(session: Option<String>) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; rak/0.1)")
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;
        Ok(Self { client, session })
    }

    fn post_graphql(&self, body: Value) -> Result<Value, String> {
        let mut req = self
            .client
            .post(GRAPHQL_URL)
            .header("Content-Type", "application/json")
            .header("Referer", "https://leetcode.com/");

        if let Some(session) = &self.session {
            req = req.header("Cookie", format!("LEETCODE_SESSION={session}"));
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

    /// Fetch all problems in a single page. LeetCode has ~3000 problems.
    pub fn fetch_all_problems(&self) -> Result<Vec<QuestionSummary>, String> {
        const PAGE_SIZE: u32 = 3000;
        const QUERY: &str = r#"
            query problemsetQuestionList($limit: Int, $skip: Int) {
              problemsetQuestionList: problemsetQuestionList(
                categorySlug: "",
                limit: $limit,
                skip: $skip,
                filters: {}
              ) {
                questions: data {
                  questionFrontendId
                  titleSlug
                  title
                  difficulty
                  topicTags { slug name }
                  status
                }
              }
            }
        "#;

        let body = json!({
            "query": QUERY,
            "variables": { "limit": PAGE_SIZE, "skip": 0 }
        });

        let resp = self.post_graphql(body)?;

        let questions = resp
            .pointer("/data/problemsetQuestionList/questions")
            .and_then(|v| v.as_array())
            .ok_or("unexpected response shape from problemsetQuestionList")?;

        let summaries: Vec<QuestionSummary> = questions
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();

        Ok(summaries)
    }

    /// Fetch full detail for a single problem by titleSlug.
    pub fn fetch_question_detail(&self, title_slug: &str) -> Result<QuestionDetail, String> {
        const QUERY: &str = r#"
            query questionData($titleSlug: String!) {
              question(titleSlug: $titleSlug) {
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
}
