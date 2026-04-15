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
    /// Raw HTML from LeetCode; convert to Markdown before writing.
    pub content: Option<String>,
    #[serde(rename = "codeSnippets")]
    pub code_snippets: Option<Vec<CodeSnippet>>,
}

impl QuestionDetail {
    pub fn cpp_snippet(&self) -> Option<&str> {
        self.code_snippets
            .as_deref()?
            .iter()
            .find(|s| s.lang_slug == "cpp")
            .map(|s| s.code.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSnippet {
    #[serde(rename = "langSlug")]
    pub lang_slug: String,
    pub code: String,
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

    #[test]
    fn cpp_snippet_finds_lang() {
        let d = QuestionDetail {
            summary: make_summary("1", "two-sum"),
            content: None,
            code_snippets: Some(vec![
                CodeSnippet {
                    lang_slug: "python3".to_string(),
                    code: "def solve(): pass".to_string(),
                },
                CodeSnippet {
                    lang_slug: "cpp".to_string(),
                    code: "class Solution {};".to_string(),
                },
            ]),
        };
        assert_eq!(d.cpp_snippet(), Some("class Solution {};"));
    }

    #[test]
    fn cpp_snippet_missing_returns_none() {
        let d = QuestionDetail {
            summary: make_summary("1", "two-sum"),
            content: None,
            code_snippets: Some(vec![CodeSnippet {
                lang_slug: "python3".to_string(),
                code: "def solve(): pass".to_string(),
            }]),
        };
        assert_eq!(d.cpp_snippet(), None);
    }
}
