use crate::table::{self, ColType};
use anyhow::{Result, anyhow, bail};
use regex::Regex;

pub(crate) struct Schema(pub String);

impl Schema {
    pub fn retype(&self, table: &str, column: &str, kind: ColType) -> Result<String> {
        let lexer = Regex::new(
            r#"(?s)--[^\n]*(?:\n|$)|/\*.*?\*/|"(?:""|[^"])*"|'(?:''|[^'])*'|`(?:``|[^`])*`|\[[^\]]*\]|[^\s(),]+|[(),]"#,
        )?;
        let tokens: Vec<_> = lexer
            .find_iter(&self.0)
            .filter(|token| !token.as_str().starts_with("--") && !token.as_str().starts_with("/*"))
            .collect();
        if tokens
            .first()
            .is_none_or(|token| !token.as_str().eq_ignore_ascii_case("CREATE"))
            || tokens
                .get(1)
                .is_none_or(|token| !token.as_str().eq_ignore_ascii_case("TABLE"))
        {
            bail!("only ordinary SQLite tables can be retyped");
        }
        let start = tokens
            .iter()
            .position(|token| token.as_str() == "(")
            .ok_or_else(|| anyhow!("table declaration has no columns"))?;
        let mut depth = 0;
        let mut first = true;
        for (i, token) in tokens.iter().enumerate().skip(start + 1) {
            match token.as_str() {
                "(" => depth += 1,
                ")" if depth > 0 => depth -= 1,
                ")" => break,
                "," if depth == 0 => first = true,
                name if first && depth == 0 => {
                    first = false;
                    let name = if name.starts_with('"') {
                        name[1..name.len() - 1].replace("\"\"", "\"")
                    } else if name.starts_with('`') {
                        name[1..name.len() - 1].replace("``", "`")
                    } else if name.starts_with('[') {
                        name[1..name.len() - 1].to_owned()
                    } else {
                        name.to_owned()
                    };
                    if !name.eq_ignore_ascii_case(column) {
                        continue;
                    }
                    let mut end = i + 1;
                    let mut nested = 0;
                    while let Some(next) = tokens.get(end) {
                        let text = next.as_str();
                        if nested == 0
                            && (text == ","
                                || text == ")"
                                || matches!(
                                    text.to_ascii_uppercase().as_str(),
                                    "CONSTRAINT"
                                        | "PRIMARY"
                                        | "NOT"
                                        | "UNIQUE"
                                        | "CHECK"
                                        | "DEFAULT"
                                        | "COLLATE"
                                        | "REFERENCES"
                                        | "GENERATED"
                                        | "AS"
                                ))
                        {
                            break;
                        }
                        if text == "(" {
                            nested += 1;
                        }
                        if text == ")" {
                            nested -= 1;
                        }
                        end += 1;
                    }
                    let until = tokens
                        .get(end)
                        .ok_or_else(|| anyhow!("incomplete column declaration"))?
                        .start();
                    return Ok(format!(
                        "CREATE TABLE {} {} {} {}",
                        table::quote(table),
                        &self.0[tokens[start].start()..token.end()],
                        kind.sql(),
                        &self.0[until..]
                    ));
                }
                _ => {}
            }
        }
        bail!("no column {column} in table declaration")
    }
}
