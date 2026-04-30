#!/usr/bin/env python3
import sys
import os
from pathlib import Path

# Try to import toml or tomllib
try:
    import tomllib as toml
except ImportError:
    try:
        import toml
    except ImportError:
        print("Error: 'toml' or 'tomllib' required. Run 'pip install toml'.")
        sys.exit(1)

def load_toml(path):
    with open(path, "rb" if "tomllib" in sys.modules else "r") as f:
        if "tomllib" in sys.modules:
            import tomllib
            return tomllib.load(f)
        else:
            import toml
            return toml.load(f)

def main():
    repo_root = Path(__file__).resolve().parents[2]
    agents_dir = repo_root / ".agents"
    
    issues_path = agents_dir / "issues.toml"
    todos_path = agents_dir / "todos.toml"
    resolved_path = agents_dir / "resolved.toml"
    
    if not issues_path.exists() or not todos_path.exists() or not resolved_path.exists():
        print("Error: Backlog files missing.")
        sys.exit(1)
        
    issues_data = load_toml(issues_path)
    todos_data = load_toml(todos_path)
    resolved_data = load_toml(resolved_path)
    
    issue_records = issues_data.get("issues", [])
    todo_records = todos_data.get("todos", [])
    issues = {i["id"]: i for i in issue_records}
    todos = {t["id"]: t for t in todo_records}
    resolved_issues = set(resolved_data.get("resolved_issues", []))
    resolved_todos = set(resolved_data.get("resolved_todos", []))
    
    errors = []

    valid_priorities = {"critical", "high", "medium", "low"}
    valid_issue_statuses = {"open", "in_progress", "blocked", "closed"}
    valid_todo_statuses = {"pending", "in_progress", "blocked", "done"}

    # 0. Check duplicate IDs before map-based validation can hide them.
    issue_ids = [str(issue.get("id", "")) for issue in issue_records]
    todo_ids = [str(todo.get("id", "")) for todo in todo_records]
    for issue_id in sorted({item for item in issue_ids if issue_ids.count(item) > 1}):
        errors.append(f"Duplicate issue id {issue_id}")
    for todo_id in sorted({item for item in todo_ids if todo_ids.count(item) > 1}):
        errors.append(f"Duplicate todo id {todo_id}")

    # 0b. Check required values and LOC invariants.
    for issue in issue_records:
        issue_id = issue.get("id", "<unknown>")
        for field in ("id", "title", "summary", "priority", "status"):
            if field not in issue:
                errors.append(f"Issue {issue_id} missing required field {field}")
        priority = issue.get("priority")
        status = issue.get("status")
        if priority is not None and priority not in valid_priorities:
            errors.append(f"Issue {issue_id} has invalid priority '{priority}'")
        if status is not None and status not in valid_issue_statuses:
            errors.append(f"Issue {issue_id} has invalid status '{status}'")

    for todo in todo_records:
        todo_id = todo.get("id", "<unknown>")
        for field in ("id", "title", "issue_ids", "priority", "status", "loc_min", "loc_expected", "loc_max"):
            if field not in todo:
                errors.append(f"Todo {todo_id} missing required field {field}")
        priority = todo.get("priority")
        status = todo.get("status")
        if priority is not None and priority not in valid_priorities:
            errors.append(f"Todo {todo_id} has invalid priority '{priority}'")
        if status is not None and status not in valid_todo_statuses:
            errors.append(f"Todo {todo_id} has invalid status '{status}'")
        loc_min = todo.get("loc_min")
        loc_expected = todo.get("loc_expected")
        loc_max = todo.get("loc_max")
        if not all(isinstance(value, int) for value in (loc_min, loc_expected, loc_max)):
            errors.append(f"Todo {todo_id} must use integer LOC estimates")
        elif not (0 <= loc_min <= loc_expected <= loc_max):
            errors.append(f"Todo {todo_id} must satisfy 0 <= loc_min <= loc_expected <= loc_max")
    
    # 1. Check resolved issues
    for issue_id in resolved_issues:
        if issue_id not in issues:
            errors.append(f"Resolved issue {issue_id} missing from issues.toml")
        elif issues[issue_id]["status"] != "closed":
            errors.append(f"Resolved issue {issue_id} is still status='{issues[issue_id]['status']}' in issues.toml")
            
    # 2. Check resolved todos
    for todo_id in resolved_todos:
        if todo_id not in todos:
            errors.append(f"Resolved todo {todo_id} missing from todos.toml")
        elif todos[todo_id]["status"] != "done":
            errors.append(f"Resolved todo {todo_id} is still status='{todos[todo_id]['status']}' in todos.toml")
            
    # 3. Check closed/done items are in resolved.toml
    for issue_id, issue in issues.items():
        if issue["status"] == "closed" and issue_id not in resolved_issues:
            errors.append(f"Issue {issue_id} is closed but missing from resolved.toml")
            
    for todo_id, todo in todos.items():
        if todo["status"] == "done" and todo_id not in resolved_todos:
            errors.append(f"Todo {todo_id} is done but missing from resolved.toml")
            
    # 4. Check todo issue_ids
    for todo_id, todo in todos.items():
        for issue_id in todo.get("issue_ids", []):
            if issue_id not in issues:
                errors.append(f"Todo {todo_id} references missing issue {issue_id}")
                
    # 5. Check metadata consistency
    issues_date = issues_data.get("meta", {}).get("updated_on")
    todos_date = todos_data.get("meta", {}).get("updated_on")
    resolved_date = resolved_data.get("meta", {}).get("updated_on")
    
    if issues_date != todos_date or issues_date != resolved_date:
        errors.append(f"Metadata date mismatch: issues={issues_date}, todos={todos_date}, resolved={resolved_date}")
        
    if errors:
        print("Backlog Consistency Errors:")
        for error in errors:
            print(f"  - {error}")
        sys.exit(1)
    else:
        print("Backlog is consistent.")
        sys.exit(0)

if __name__ == "__main__":
    main()
