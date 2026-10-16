use serde::{Deserialize, Serialize};

/// Lightweight summary from the problem list API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionSummary {
    #[serde(rename = "questionFrontendId")]
    pub frontend_id: String,
    #[serde(rename = "titleSlug")]
    pub title_slug: String,
    pub title: String,
    pub difficulty: String, // "Easy" | "Medium" | "Hard"
    #[serde(rename = "topicTags")]
    pub topic_tags: Vec<TopicTag>,
    /// null when unauthenticated
    pub status: Option<String>, // "ac" | "notac" | null
}

impl QuestionSummary {
    /// Full folder name: "0001.two-sum"
    pub fn folder_name(&self) -> String {
        let n: u32 = self.frontend_id.parse().unwrap_or(0);
        format!("{:0>4}.{}", n, self.title_slug)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicTag {
    pub slug: String,
    pub name: String,
}

/// Full problem detail including HTML content and code snippets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionDetail {
    #[serde(flatten)]
    pub summary: QuestionSummary,
    /// Internal LeetCode question ID (used for submission, distinct from frontend ID).
    #[serde(rename = "questionId", default)]
    pub question_id: String,
    /// True for Premium-only problems; content and snippets are withheld without a subscription.
    #[serde(rename = "isPaidOnly", default)]
    pub is_paid_only: bool,
    /// Raw HTML from LeetCode; convert to Markdown before writing.
    pub content: Option<String>,
    #[serde(rename = "codeSnippets")]
    pub code_snippets: Option<Vec<CodeSnippet>>,
    #[serde(rename = "exampleTestcaseList", default)]
    pub example_testcase_list: Option<Vec<String>>,
    #[serde(rename = "exampleTestcases", default)]
    pub example_testcases: Option<String>,
    #[serde(rename = "sampleTestCase", default)]
    pub sample_test_case: Option<String>,
    /// JSON-encoded function signature metadata.
    #[serde(rename = "metaData", default)]
    pub meta_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSnippet {
    #[serde(rename = "langSlug")]
    pub lang_slug: String,
    pub code: String,
}

/// Result from `/submissions/detail/{id}/check/`.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckResult {
    /// "STARTED" | "SUCCESS" — keep polling until "SUCCESS"
    pub state: String,
    /// "Accepted", "Wrong Answer", "Time Limit Exceeded", "Compile Error", …
    #[serde(default)]
    pub status_msg: String,
    #[serde(default)]
    pub run_success: bool,
    pub total_correct: Option<u32>,
    pub total_testcases: Option<u32>,
    pub status_runtime: Option<String>,
    pub status_memory: Option<String>,
    pub compile_error: Option<String>,
    pub full_compile_error: Option<String>,
}

/// On-disk cache envelope.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProblemCache {
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub problems: Vec<QuestionSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_summary(id: &str, slug: &str) -> QuestionSummary {
        QuestionSummary {
            frontend_id: id.to_string(),
            title_slug: slug.to_string(),
            title: slug.to_string(),
            difficulty: "Easy".to_string(),
            topic_tags: vec![],
            status: None,
        }
    }

    #[test]
    fn folder_name_zero_pads() {
        let s = make_summary("1", "two-sum");
        assert_eq!(s.folder_name(), "0001.two-sum");
    }

    #[test]
    fn folder_name_large_id() {
        let s = make_summary("1234", "some-problem");
        assert_eq!(s.folder_name(), "1234.some-problem");
    }
}
