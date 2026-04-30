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
    
    issues = {i["id"]: i for i in issues_data.get("issues", [])}
    todos = {t["id"]: t for t in todos_data.get("todos", [])}
    resolved_issues = set(resolved_data.get("resolved_issues", []))
    resolved_todos = set(resolved_data.get("resolved_todos", []))
    
    errors = []
    
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
