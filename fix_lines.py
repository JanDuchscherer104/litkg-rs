import sys

def insert_at(file_path, replacements):
    with open(file_path, "r") as f:
        lines = f.readlines()
    for line_num, text in replacements.items():
        idx = line_num - 1
        lines[idx] = lines[idx].replace("{", "{ " + text + ",")
    with open(file_path, "w") as f:
        f.writelines(lines)

parsed_paper_text = "kind: crate::model::DocumentKind::Literature"
repo_config_text = "project: None, sources: std::collections::BTreeMap::new(), representation: None, backends: None, storage: None"

insert_at("crates/litkg-core/src/enrich.rs", {802: parsed_paper_text})
insert_at("crates/litkg-core/src/inspect.rs", {
    1274: repo_config_text,
    1353: parsed_paper_text, 1371: parsed_paper_text, 1391: parsed_paper_text,
    1523: parsed_paper_text, 1573: parsed_paper_text, 1622: parsed_paper_text,
    1649: parsed_paper_text, 1712: parsed_paper_text, 1743: parsed_paper_text,
    1826: parsed_paper_text, 1897: parsed_paper_text
})
insert_at("crates/litkg-core/src/materialize.rs", {407: repo_config_text, 429: parsed_paper_text})
insert_at("crates/litkg-core/src/memory.rs", {1169: repo_config_text, 1188: parsed_paper_text})
insert_at("crates/litkg-core/src/notebook.rs", {240: parsed_paper_text})
insert_at("crates/litkg-core/src/registry.rs", {329: repo_config_text})
insert_at("crates/litkg-core/src/tabular.rs", {785: parsed_paper_text})
insert_at("crates/litkg-core/src/tex.rs", {614: repo_config_text})
