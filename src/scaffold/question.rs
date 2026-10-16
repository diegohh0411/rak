//! Question data needed to generate leetgo-style files, plus the pieces of
//! leetgo's `QuestionData` logic (metadata, formatted content, example outputs).

use regex::Regex;
use serde::Deserialize;

use super::{markdown, wordwrap};
use crate::leetcode::models::QuestionDetail;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetaDataParam {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub ty: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetaDataReturn {
    #[serde(rename = "type", default)]
    pub ty: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetaDataOutput {
    #[serde(default)]
    pub paramindex: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetaDataMethod {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub params: Vec<MetaDataParam>,
    #[serde(rename = "return", default)]
    pub ret: MetaDataReturn,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetaDataConstructor {
    #[serde(default)]
    pub params: Vec<MetaDataParam>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetaData {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub params: Vec<MetaDataParam>,
    #[serde(rename = "return")]
    pub ret: Option<MetaDataReturn>,
    pub output: Option<MetaDataOutput>,
    #[serde(default)]
    pub systemdesign: bool,
    #[serde(default)]
    pub classname: String,
    #[serde(default)]
    pub constructor: MetaDataConstructor,
    #[serde(default)]
    pub methods: Vec<MetaDataMethod>,
    #[serde(default)]
    pub manual: bool,
}

/// LeetCode's type names are inconsistent; normalize like leetgo does.
fn normalize_type(ty: &str) -> String {
    if let Some(inner) = ty.strip_prefix("list<").and_then(|s| s.strip_suffix('>')) {
        return normalize_type(inner) + "[]";
    }
    match ty {
        "String" => "string".into(),
        "" => "void".into(),
        _ => ty.into(),
    }
}

impl MetaData {
    pub fn parse(raw: &str) -> MetaData {
        let mut m: MetaData = serde_json::from_str(raw).unwrap_or_default();
        for p in m.params.iter_mut() {
            p.ty = normalize_type(&p.ty);
        }
        if let Some(r) = m.ret.as_mut() {
            r.ty = normalize_type(&r.ty);
        }
        for method in m.methods.iter_mut() {
            for p in method.params.iter_mut() {
                p.ty = normalize_type(&p.ty);
            }
            method.ret.ty = normalize_type(&method.ret.ty);
        }
        m
    }

    pub fn n_arg(&self) -> usize {
        if self.systemdesign { 2 } else { self.params.len() }
    }

    pub fn result_type(&self) -> String {
        if let Some(r) = &self.ret
            && r.ty != "void"
        {
            return r.ty.clone();
        }
        match &self.output {
            Some(o) => self.params.get(o.paramindex).map(|p| p.ty.clone()).unwrap_or_default(),
            None => String::new(),
        }
    }
}

pub struct Question<'a> {
    pub detail: &'a QuestionDetail,
    pub meta: MetaData,
}

impl<'a> Question<'a> {
    pub fn new(detail: &'a QuestionDetail) -> Self {
        let meta = MetaData::parse(detail.meta_data.as_deref().unwrap_or(""));
        Question { detail, meta }
    }

    pub fn url(&self) -> String {
        format!("https://leetcode.com/problems/{}/", self.detail.summary.title_slug)
    }

    pub fn content(&self) -> &str {
        self.detail.content.as_deref().unwrap_or("")
    }

    pub fn code_snippet(&self, lang: &str) -> Option<&str> {
        self.detail
            .code_snippets
            .as_deref()?
            .iter()
            .find(|s| s.lang_slug == lang)
            .map(|s| s.code.as_str())
    }

    /// leetgo `GetFormattedContent`: markdown, wrapped at 100 columns.
    pub fn formatted_content(&self) -> String {
        let md = markdown::html_to_markdown(self.content());
        let wrapped = wordwrap::wrap(&md, 100);
        ensure_trailing_newline(condense_empty_lines(&wrapped))
    }

    /// leetgo `GetExampleTestCases`.
    pub fn example_test_cases(&self) -> Vec<String> {
        let d = self.detail;
        if let Some(list) = d.example_testcase_list.as_ref().filter(|l| !l.is_empty()) {
            return list.iter().flat_map(|c| c.split('\n').map(String::from)).collect();
        }
        if let Some(s) = d.example_testcases.as_deref().filter(|s| !s.is_empty()) {
            return s.split('\n').map(String::from).collect();
        }
        if let Some(s) = d.sample_test_case.as_deref().filter(|s| !s.is_empty()) {
            return s.split('\n').map(String::from).collect();
        }
        Vec::new()
    }

    /// leetgo `ParseExampleOutputs`: scrape `Output:` values out of the HTML.
    pub fn example_outputs(&self) -> Vec<String> {
        let pat = Regex::new(
            r"(?si)<strong>(?:Output|输出)[:：]?\s*</strong>\s*(?:<span[^>]*>)?\s*([^<]+)\s*(?:</span>)?",
        )
        .expect("valid regex");
        pat.captures_iter(self.content())
            .map(|cap| {
                let out = cap[1].trim();
                let out = out.strip_prefix("<code>").unwrap_or(out);
                let out = out.strip_suffix("</pre>").unwrap_or(out);
                let out = out.strip_suffix("</code>").unwrap_or(out);
                let out = decode_entities(out);
                out.split('\n').map(str::trim).collect::<String>()
            })
            .collect()
    }
}

pub fn condense_empty_lines(s: &str) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let mut kept = Vec::with_capacity(lines.len());
    for i in 0..lines.len() {
        if i == 0 || !lines[i].is_empty() || !lines[i - 1].is_empty() {
            kept.push(lines[i]);
        }
    }
    kept.join("\n")
}

pub fn ensure_trailing_newline(mut s: String) -> String {
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{00A0}'),
            e if e.starts_with("#x") || e.starts_with("#X") => {
                u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32)
            }
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_list_types() {
        assert_eq!(normalize_type("list<list<integer>>"), "integer[][]");
        assert_eq!(normalize_type("String"), "string");
        assert_eq!(normalize_type(""), "void");
    }

    #[test]
    fn condenses_blank_lines() {
        assert_eq!(condense_empty_lines("a\n\n\n\nb"), "a\n\nb");
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(decode_entities("&quot;a&quot; &lt; &#39;b&#39;"), "\"a\" < 'b'");
    }
}
