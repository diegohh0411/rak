//! C++ `solution.cpp` generation, matching leetgo's cpp generator: the
//! LeetCode snippet between `@lc code` markers plus a `main()` that reads
//! test input through `LC_IO.h`. leetgo's "Created by / leetgo: version"
//! header lines are intentionally omitted.

use super::question::{MetaDataParam, Question, condense_empty_lines, ensure_trailing_newline};

pub const LC_IO_H: &str = include_str!("LC_IO.h");
pub const STDCXX_H: &str = include_str!("stdc++.h");

pub const CODE_BEGIN_MARKER: &str = "@lc code=begin";
pub const CODE_END_MARKER: &str = "@lc code=end";

const MANUAL_WARNING: &str =
    "Warning: this is a manual question, the generated test code may be incorrect.";

pub fn generate(q: &Question, snippet: &str) -> String {
    let code = remove_useless_comments(snippet);
    let content = format!(
        "// {url}\n\n\
         #include <bits/stdc++.h>\n\
         #include \"LC_IO.h\"\n\
         using namespace std;\n\n\
         // {begin}\n\n\
         {code}\n\n\
         // {end}\n\n\
         {test}\n",
        url = q.url(),
        begin = CODE_BEGIN_MARKER,
        end = CODE_END_MARKER,
        test = test_content(q),
    );
    ensure_trailing_newline(condense_empty_lines(&content))
}

/// Extract the code between the `@lc code` markers; files without markers are
/// returned unchanged.
pub fn extract_solution(source: &str) -> String {
    let mut lines = Vec::new();
    let mut inside = false;
    let mut found = false;
    for line in source.lines() {
        if line.contains(CODE_BEGIN_MARKER) {
            inside = true;
            found = true;
            continue;
        }
        if line.contains(CODE_END_MARKER) {
            inside = false;
            continue;
        }
        if inside {
            lines.push(line);
        }
    }
    if found { lines.join("\n").trim().to_string() } else { source.to_string() }
}

/// leetgo's `removeUselessComments` modifier.
fn remove_useless_comments(code: &str) -> String {
    let lines: Vec<&str> = code.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let next = lines.get(i + 1).copied().unwrap_or("");
        if line.starts_with("/**")
            && (next.contains("object will be instantiated and called")
                || next.contains("Definition for"))
        {
            while i < lines.len() && !lines[i].ends_with("*/") {
                i += 1;
            }
            i += 1;
            continue;
        }
        out.push(line);
        i += 1;
    }
    out.join("\n")
}

fn cpp_type(t: &str) -> (usize, &'static str) {
    let dims = t.matches("[]").count();
    let base = match t.replace("[]", "").as_str() {
        "void" => "void",
        "integer" => "int",
        "long" => "int64_t",
        "string" => "string",
        "double" => "double",
        "ListNode" => "ListNode*",
        "TreeNode" => "TreeNode*",
        "boolean" => "bool",
        "character" => "char",
        _ => "",
    };
    (dims, base)
}

fn deserialize_line(p: &MetaDataParam, input: &str) -> String {
    let (dims, base) = cpp_type(&p.ty);
    let ty = format!("{}{}{}", "vector<".repeat(dims), base, ">".repeat(dims));
    format!("{ty} {} = LeetCodeIO::deserialize<{ty}>({input});", p.name)
}

fn param_list(params: &[MetaDataParam]) -> String {
    params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
}

fn test_content(q: &Question) -> String {
    let m = &q.meta;
    let mut body = String::new();

    // scan
    if m.systemdesign {
        body.push_str("\tauto method_names = LeetCodeIO::deserialize<vector<string>>(cin);\n");
        body.push_str("\tauto params = LeetCodeIO::split_array(cin);\n");
    } else {
        for p in &m.params {
            body.push_str(&format!("\t{}\n", deserialize_line(p, "cin")));
        }
    }
    body.push('\n');

    // init
    if m.systemdesign {
        body.push_str(&format!("\tunique_ptr<{}> obj;\n", m.classname));
    } else {
        body.push_str("\tSolution obj;\n");
    }

    // call
    if !m.systemdesign {
        let args = param_list(&m.params);
        if m.ret.as_ref().is_some_and(|r| r.ty != "void") {
            body.push_str(&format!("\tauto res = obj.{}({args});\n", m.name));
        } else {
            body.push_str(&format!("\tobj.{}({args});\n", m.name));
        }
    } else {
        let scan = |params: &[MetaDataParam]| -> String {
            params
                .iter()
                .enumerate()
                .map(|(i, p)| format!("\t\t\t{}\n", deserialize_line(p, &format!("method_params, {i}"))))
                .collect()
        };
        let lambda_param = |params: &[MetaDataParam]| {
            if params.is_empty() { "const vector<string> &" } else { "const vector<string> &method_params" }
        };
        body.push_str("\tvector<string> output;\n");
        body.push_str("\tconst unordered_map<string, function<void(const vector<string> &)>> methods = {\n");
        let ctor = &m.constructor.params;
        body.push_str(&format!("\t\t{{ \"{}\", [&]({}) {{\n", m.classname, lambda_param(ctor)));
        body.push_str(&scan(ctor));
        body.push_str(&format!("\t\t\tobj = make_unique<{}>({});\n", m.classname, param_list(ctor)));
        body.push_str("\t\t\toutput.push_back(\"null\");\n\t\t} },\n");
        for method in &m.methods {
            body.push_str(&format!("\t\t{{ \"{}\", [&]({}) {{\n", method.name, lambda_param(&method.params)));
            body.push_str(&scan(&method.params));
            let call = format!("obj->{}({})", method.name, param_list(&method.params));
            if !method.ret.ty.is_empty() && method.ret.ty != "void" {
                body.push_str(&format!("\t\t\toutput.push_back(LeetCodeIO::serialize({call}));\n"));
            } else {
                body.push_str(&format!("\t\t\t{call};\n\t\t\toutput.push_back(\"null\");\n"));
            }
            body.push_str("\t\t} },\n");
        }
        body.push_str("\t};\n");
        body.push_str(
            "\tif (method_names.size() != params.size()) {\n\
             \t\tthrow LeetCodeIO::Error(\"method and parameter counts differ\");\n\
             \t}\n\
             \tfor (size_t i = 0; i < method_names.size(); ++i) {\n\
             \t\tauto method_params = LeetCodeIO::split_array(params[i]);\n\
             \t\tmethods.at(method_names[i])(method_params);\n\
             \t}\n",
        );
    }

    // print
    if m.systemdesign {
        body.push_str("\tcout << \"\\noutput: \" << LeetCodeIO::join_array(output) << '\\n';\n");
    } else {
        body.push_str("\n\tstringstream out_stream;\n");
        if m.ret.as_ref().is_some_and(|r| r.ty != "void") {
            body.push_str("\tLeetCodeIO::print(out_stream, res);\n");
        } else if let Some(o) = &m.output {
            let name = m.params.get(o.paramindex).map(|p| p.name.as_str()).unwrap_or("");
            body.push_str(&format!("\tLeetCodeIO::print(out_stream, {name});\n"));
        } else {
            body.push_str("\tout_stream << \"null\";\n");
        }
        body.push_str("\tcout << \"\\noutput: \" << out_stream.rdbuf() << '\\n';\n");
    }

    let indented: Vec<String> = body
        .strip_suffix('\n')
        .unwrap_or(&body)
        .split('\n')
        .map(|l| if l.is_empty() { String::new() } else { format!("\t{l}") })
        .collect();
    let test = format!(
        "int main() {{\n\
         \tios_base::sync_with_stdio(false);\n\
         \ttry {{\n\
         {}\n\
         \t}} catch (const LeetCodeIO::Error &error) {{\n\
         \t\tcerr << \"LC_IO: \" << error.what() << '\\n';\n\
         \t\treturn 2;\n\
         \t}}\n\
         \treturn 0;\n\
         }}",
        indented.join("\n")
    );
    if m.manual { format!("// {MANUAL_WARNING}\n{test}") } else { test }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_code_between_markers() {
        let src = "// x\n// @lc code=begin\n\nclass Solution {};\n\n// @lc code=end\nint main() {}\n";
        assert_eq!(extract_solution(src), "class Solution {};");
    }

    #[test]
    fn extract_without_markers_returns_source() {
        assert_eq!(extract_solution("class Solution {};"), "class Solution {};");
    }

    #[test]
    fn strips_definition_comment() {
        let code = "/**\n * Definition for singly-linked list.\n * struct ListNode {};\n */\nclass Solution {};";
        assert_eq!(remove_useless_comments(code), "class Solution {};");
    }
}
