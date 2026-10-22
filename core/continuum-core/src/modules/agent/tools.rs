//! Agent tool surface: tool-call parsing from LLM text and the file/search/command/git tool implementations.

use super::*;

/// Static regexes for tool call parsing (compiled once)
static TOOL_BLOCK_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"```tool\s*\n?([\s\S]*?)```").unwrap());
static INLINE_TOOL_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r#"\{"name":\s*"(\w+)",\s*"arguments":\s*(\{[^}]+\})\}"#).unwrap()
});

/// Parse tool calls from LLM response
pub(super) fn parse_tool_calls(response: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();

    for cap in TOOL_BLOCK_RE.captures_iter(response) {
        if let Some(json_str) = cap.get(1) {
            if let Ok(parsed) = serde_json::from_str::<ToolCall>(json_str.as_str().trim()) {
                calls.push(parsed);
            }
        }
    }

    for cap in INLINE_TOOL_RE.captures_iter(response) {
        if let (Some(name), Some(args)) = (cap.get(1), cap.get(2)) {
            if let Ok(arguments) = serde_json::from_str::<Value>(args.as_str()) {
                let call = ToolCall {
                    name: name.as_str().to_string(),
                    arguments,
                };
                // Avoid duplicates
                if !calls
                    .iter()
                    .any(|c| c.name == call.name && c.arguments == call.arguments)
                {
                    calls.push(call);
                }
            }
        }
    }

    calls
}

/// Execute a tool call (runs on Rayon thread pool)
pub(super) fn execute_tool(call: &ToolCall, working_dir: &Path) -> ToolResult {
    match call.name.as_str() {
        "read_file" => tool_read_file(call, working_dir),
        "write_file" => tool_write_file(call, working_dir),
        "edit_file" => tool_edit_file(call, working_dir),
        "search_files" => tool_search_files(call, working_dir),
        "list_files" => tool_list_files(call, working_dir),
        "run_command" => tool_run_command(call, working_dir),
        "git_status" => tool_git_status(working_dir),
        "git_diff" => tool_git_diff(call, working_dir),
        "complete" | "give_up" => ToolResult {
            success: true,
            output: "Acknowledged".to_string(),
            error: None,
        },
        _ => ToolResult {
            success: false,
            output: format!("Unknown tool: {}", call.name),
            error: Some(format!("Unknown tool: {}", call.name)),
        },
    }
}

// ============================================================================
// TOOL IMPLEMENTATIONS
// ============================================================================

fn tool_read_file(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let path = match call.arguments.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                output: "Missing path argument".to_string(),
                error: Some("Missing path".to_string()),
            };
        }
    };

    let full_path = working_dir.join(path);
    if !full_path.exists() {
        return ToolResult {
            success: false,
            output: format!("File not found: {}", path),
            error: Some("File not found".to_string()),
        };
    }

    match std::fs::read_to_string(&full_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = call
                .arguments
                .get("start_line")
                .and_then(|v| v.as_u64())
                .map(|n| (n as usize).saturating_sub(1))
                .unwrap_or(0);
            let end = call
                .arguments
                .get("end_line")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(lines.len());

            let selected: String = lines[start..end.min(lines.len())]
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{:4}: {}", start + i + 1, line))
                .collect::<Vec<_>>()
                .join("\n");

            ToolResult {
                success: true,
                output: format!(
                    "File: {} ({} lines, showing {}-{})\n\n{}",
                    path,
                    lines.len(),
                    start + 1,
                    end.min(lines.len()),
                    selected
                ),
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Error reading file: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_write_file(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let path = match call.arguments.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                output: "Missing path argument".to_string(),
                error: Some("Missing path".to_string()),
            };
        }
    };

    let content = match call.arguments.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                success: false,
                output: "Missing content argument".to_string(),
                error: Some("Missing content".to_string()),
            };
        }
    };

    let full_path = working_dir.join(path);

    // Create parent directories
    if let Some(parent) = full_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return ToolResult {
                success: false,
                output: format!("Failed to create directories: {}", e),
                error: Some(e.to_string()),
            };
        }
    }

    let existed = full_path.exists();
    match std::fs::write(&full_path, content) {
        Ok(_) => ToolResult {
            success: true,
            output: format!(
                "{} {} ({} bytes, {} lines)",
                if existed { "Updated" } else { "Created" },
                path,
                content.len(),
                content.lines().count()
            ),
            error: None,
        },
        Err(e) => ToolResult {
            success: false,
            output: format!("Error writing file: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_edit_file(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let path = match call.arguments.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                output: "Missing path argument".to_string(),
                error: Some("Missing path".to_string()),
            };
        }
    };

    let search = match call.arguments.get("search").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                success: false,
                output: "Missing search argument".to_string(),
                error: Some("Missing search".to_string()),
            };
        }
    };

    let replace = match call.arguments.get("replace").and_then(|v| v.as_str()) {
        Some(r) => r,
        None => {
            return ToolResult {
                success: false,
                output: "Missing replace argument".to_string(),
                error: Some("Missing replace".to_string()),
            };
        }
    };

    let full_path = working_dir.join(path);
    if !full_path.exists() {
        return ToolResult {
            success: false,
            output: format!("File not found: {}", path),
            error: Some("File not found".to_string()),
        };
    }

    match std::fs::read_to_string(&full_path) {
        Ok(content) => {
            let count = content.matches(search).count();
            if count == 0 {
                return ToolResult {
                    success: false,
                    output: format!(
                        "Search string not found in {}. Make sure to use exact text including whitespace.",
                        path
                    ),
                    error: Some("Search string not found".to_string()),
                };
            }
            if count > 1 {
                return ToolResult {
                    success: false,
                    output: format!(
                        "Search string found {} times in {}. Use a more specific search to match exactly one location.",
                        count, path
                    ),
                    error: Some("Multiple matches".to_string()),
                };
            }

            let new_content = content.replace(search, replace);
            match std::fs::write(&full_path, &new_content) {
                Ok(_) => ToolResult {
                    success: true,
                    output: format!(
                        "Edited {}: replaced {} chars with {} chars",
                        path,
                        search.len(),
                        replace.len()
                    ),
                    error: None,
                },
                Err(e) => ToolResult {
                    success: false,
                    output: format!("Error writing file: {}", e),
                    error: Some(e.to_string()),
                },
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Error reading file: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_search_files(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let pattern = match call.arguments.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                output: "Missing pattern argument".to_string(),
                error: Some("Missing pattern".to_string()),
            };
        }
    };

    let max_results = call
        .arguments
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(50);

    let file_glob = call
        .arguments
        .get("file_glob")
        .and_then(|v| v.as_str())
        .map(|g| format!("--include=\"{}\"", g))
        .unwrap_or_default();

    let cmd = format!(
        "grep -rn {} -E \"{}\" . 2>/dev/null | head -{}",
        file_glob,
        pattern.replace('"', "\\\""),
        max_results
    );

    match Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .current_dir(working_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout.lines().collect();

            ToolResult {
                success: true,
                output: format!(
                    "Found {} matches:\n{}",
                    lines.len(),
                    if stdout.is_empty() {
                        "No matches found".to_string()
                    } else {
                        stdout.to_string()
                    }
                ),
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Search error: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_list_files(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let dir_path = call
        .arguments
        .get("path")
        .and_then(|v| v.as_str())
        .map(|p| working_dir.join(p))
        .unwrap_or_else(|| working_dir.to_path_buf());

    let depth = call
        .arguments
        .get("depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(3);

    let pattern = call
        .arguments
        .get("pattern")
        .and_then(|v| v.as_str())
        .map(|p| format!("-name \"{}\"", p))
        .unwrap_or_default();

    let cmd = format!(
        "find \"{}\" -maxdepth {} -type f {} 2>/dev/null | head -100",
        dir_path.display(),
        depth,
        pattern
    );

    match Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .current_dir(working_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut paths: Vec<String> = stdout
                .lines()
                .filter(|l| !l.is_empty())
                .filter_map(|l| {
                    // Make paths relative to working_dir
                    let p = Path::new(l);
                    p.strip_prefix(working_dir)
                        .map(|r| r.to_string_lossy().to_string())
                        .ok()
                        .or_else(|| Some(l.to_string()))
                })
                .collect();
            paths.sort();

            ToolResult {
                success: true,
                output: format!(
                    "Files in {}:\n{}",
                    call.arguments
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("."),
                    if paths.is_empty() {
                        "No files found".to_string()
                    } else {
                        paths.join("\n")
                    }
                ),
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("List error: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_run_command(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let command = match call.arguments.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                success: false,
                output: "Missing command argument".to_string(),
                error: Some("Missing command".to_string()),
            };
        }
    };

    let _timeout_ms = call
        .arguments
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(60000);

    // Execute with timeout using std::process for Rayon compatibility
    match Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(working_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let success = output.status.success();

            if success {
                ToolResult {
                    success: true,
                    output: if stdout.is_empty() {
                        "Command completed (no output)".to_string()
                    } else {
                        stdout.chars().take(5000).collect()
                    },
                    error: None,
                }
            } else {
                ToolResult {
                    success: false,
                    output: format!(
                        "Exit code: {:?}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                        output.status.code(),
                        stdout.chars().take(2500).collect::<String>(),
                        stderr.chars().take(2500).collect::<String>()
                    ),
                    error: Some(format!(
                        "Command failed with exit code {:?}",
                        output.status.code()
                    )),
                }
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Execution error: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_git_status(working_dir: &Path) -> ToolResult {
    match Command::new("git")
        .args(["status", "--short"])
        .current_dir(working_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            ToolResult {
                success: true,
                output: if stdout.is_empty() {
                    "Working tree clean".to_string()
                } else {
                    stdout.to_string()
                },
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Git error: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_git_diff(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let mut args = vec!["diff"];

    if call
        .arguments
        .get("staged")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        args.push("--staged");
    }

    let path = call.arguments.get("path").and_then(|v| v.as_str());

    let mut cmd = Command::new("git");
    cmd.args(&args).current_dir(working_dir);

    if let Some(p) = path {
        cmd.arg("--").arg(p);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            ToolResult {
                success: true,
                output: if stdout.is_empty() {
                    "No changes".to_string()
                } else {
                    stdout.chars().take(5000).collect()
                },
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Git error: {}", e),
            error: Some(e.to_string()),
        },
    }
}
