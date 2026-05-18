import re

with open("crates/litkg-cli/tests/inspect_cli.rs", "r") as f:
    content = f.read()

# Fix subcommands used in .args([...])
content = content.replace('        .args([\n            "capabilities",', '        .args([\n            "info",\n            "capabilities",')
content = content.replace('        .args([\n            "stats",', '        .args([\n            "info",\n            "stats",')
content = content.replace('        .args([\n            "context-pack",', '        .args([\n            "info",\n            "context-pack",')
content = content.replace('        .args([\n            "search",', '        .args([\n            "lit",\n            "search",')
content = content.replace('        .args([\n            "show-paper",', '        .args([\n            "lit",\n            "show",')
# The original code might have `show-paper`

with open("crates/litkg-cli/tests/inspect_cli.rs", "w") as f:
    f.write(content)
