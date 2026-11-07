//! Parsing of `config.agents.recognized` from the Lua config: extra agents for the sidebar
//! "Agents" list, merged over the built-in defaults (see `term::agents`).

use mlua::prelude::*;

use crate::term::agents::{parse_hex_color, RecognizedAgent};

/// Valid entries of `recognized` in `agents_table`; invalid ones are skipped with a warning.
pub fn parse_recognized(agents_table: &LuaTable) -> Vec<RecognizedAgent> {
    let Ok(Some(list)) = agents_table.get::<Option<LuaTable>>("recognized") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, entry) in list.sequence_values::<LuaTable>().enumerate() {
        match entry.ok().and_then(|t| parse_entry(&t)) {
            Some(agent) => out.push(agent),
            None => log::warn!(
                "agents.recognized[{}] ignored: needs name and a non-empty process list",
                i + 1
            ),
        }
    }
    out
}

fn parse_entry(t: &LuaTable) -> Option<RecognizedAgent> {
    let name = t.get::<Option<String>>("name").ok()??.trim().to_string();
    let procs: LuaTable = t.get::<Option<LuaTable>>("process").ok()??;
    let process: Vec<String> = procs
        .sequence_values::<String>()
        .filter_map(Result::ok)
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    // An unparsable color is ignored (the agent still works), not a reason to drop the entry.
    let color = t
        .get::<Option<String>>("color")
        .ok()
        .flatten()
        .and_then(|c| parse_hex_color(&c));
    (!name.is_empty() && !process.is_empty()).then_some(RecognizedAgent {
        name,
        process,
        color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Vec<RecognizedAgent> {
        let lua = Lua::new();
        let t: LuaTable = lua.load(src).eval().unwrap();
        parse_recognized(&t)
    }

    #[test]
    fn valid_entries_are_parsed() {
        let got = parse(
            r#"return { recognized = { { name = "Gemini", process = { "gemini", "gemini-cli" } } } }"#,
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "Gemini");
        assert_eq!(got[0].process, vec!["gemini", "gemini-cli"]);
    }

    #[test]
    fn color_is_parsed_and_bad_color_is_ignored() {
        let got = parse(
            r##"return { recognized = {
                { name = "A", process = { "a" }, color = "#102030" },
                { name = "B", process = { "b" }, color = "nope" },
            } }"##,
        );
        assert_eq!(got[0].color, Some([0x10, 0x20, 0x30]));
        assert_eq!(got[1].color, None);
    }

    #[test]
    fn invalid_entries_are_skipped() {
        let got = parse(
            r#"return { recognized = {
                { name = "", process = { "x" } },
                { name = "NoProc", process = {} },
                { process = { "y" } },
                "junk",
                { name = "Ok", process = { "ok" } },
            } }"#,
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "Ok");
    }

    #[test]
    fn absent_key_is_empty() {
        assert!(parse("return {}").is_empty());
    }
}
