use anyhow::Result;
use clap::{Parser, Subcommand};
use std::env;

#[derive(Parser)]
#[command(name = "jjagent")]
#[command(about = "JJ Claude Code - Manage jj changesets for Claude sessions")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Claude Code integration
    #[command(subcommand, alias = "c")]
    Claude(ClaudeCommands),
    /// Split a change into a new session part before @
    Split {
        /// The Claude session ID or jj reference to split (e.g., session ID, change ID, or revset)
        #[arg(value_name = "SESSION_ID_OR_REF")]
        reference: String,
    },
    /// Get the jj change ID for a Claude session
    #[command(name = "change-id")]
    ChangeId {
        /// The Claude session ID
        #[arg(value_name = "SESSION_ID")]
        session_id: String,
    },
    /// Update the description of a session's commit while preserving trailers
    Describe {
        /// The Claude session ID
        #[arg(value_name = "SESSION_ID")]
        session_id: String,
        /// The new commit message (without trailers)
        #[arg(short, long, value_name = "MESSAGE")]
        message: String,
    },
}

#[derive(Subcommand)]
enum ClaudeCommands {
    /// Print Claude Code settings JSON
    Settings,
    /// Claude Code hooks for jj integration
    #[command(subcommand)]
    Hooks(HookCommands),
}

#[derive(Subcommand)]
enum HookCommands {
    /// Handle PreToolUse hook
    #[command(name = "PreToolUse")]
    PreToolUse,
    /// Handle PostToolUse hook
    #[command(name = "PostToolUse")]
    PostToolUse,
    /// Handle Stop hook
    #[command(name = "Stop")]
    Stop,
    /// Handle UserPromptSubmit hook
    #[command(name = "UserPromptSubmit")]
    UserPromptSubmit,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let result = run_command(cli);

    // Log any errors that occurred
    if let Err(ref e) = result {
        jjagent::logger::logger().log_error(e, "main");
    }

    result
}

fn run_command(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Claude(claude_cmd) => {
            // Handle Settings command outside of jj repo check
            if let ClaudeCommands::Settings = claude_cmd {
                let settings = jjagent::format_claude_settings()?;
                println!("{}", settings);
                return Ok(());
            }

            match claude_cmd {
                ClaudeCommands::Settings => unreachable!(),
                ClaudeCommands::Hooks(hook_cmd) => {
                    // Check if hooks are disabled
                    if env::var("JJAGENT_DISABLE").unwrap_or_default() == "1" {
                        eprintln!("jjagent: Disabled via JJAGENT_DISABLE=1");
                        return Ok(());
                    }

                    let hook_name = match hook_cmd {
                        HookCommands::PreToolUse => "PreToolUse",
                        HookCommands::PostToolUse => "PostToolUse",
                        HookCommands::Stop => "Stop",
                        HookCommands::UserPromptSubmit => "UserPromptSubmit",
                    };
                    eprintln!("jjagent: {} hook called", hook_name);

                    // Handle hooks that return HookResponse directly
                    match hook_cmd {
                        HookCommands::UserPromptSubmit => {
                            let input = jjagent::hooks::HookInput::from_stdin()?;
                            match jjagent::hooks::handle_user_prompt_submit_hook(&input) {
                                Ok(response) => {
                                    response.output();
                                }
                                Err(e) => {
                                    let response =
                                        jjagent::hooks::HookResponse::stop(e.to_string());
                                    response.output();
                                    return Err(e);
                                }
                            }
                        }
                        _ => {
                            // PreToolUse, PostToolUse, Stop return Result<()>
                            let result = match hook_cmd {
                                HookCommands::PreToolUse => {
                                    let input = jjagent::hooks::HookInput::from_stdin()?;
                                    jjagent::hooks::handle_pretool_hook(input)
                                }
                                HookCommands::PostToolUse => {
                                    let input = jjagent::hooks::HookInput::from_stdin()?;
                                    jjagent::hooks::handle_posttool_hook(input)
                                }
                                HookCommands::Stop => {
                                    let input = jjagent::hooks::HookInput::from_stdin()?;
                                    jjagent::hooks::handle_stop_hook(input)
                                }
                                _ => unreachable!(),
                            };

                            // Output JSON response based on result
                            match result {
                                Ok(_) => {
                                    let response =
                                        jjagent::hooks::HookResponse::continue_execution();
                                    response.output();
                                }
                                Err(e) => {
                                    let response =
                                        jjagent::hooks::HookResponse::stop(e.to_string());
                                    response.output();
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
            }
        }
        Commands::Split { reference } => {
            jjagent::split_change(&reference)?;
        }
        Commands::ChangeId { session_id } => {
            match jjagent::jj::find_session_change_anywhere(&session_id)? {
                Some(commit) => {
                    println!("{}", commit.change_id);
                }
                None => {
                    anyhow::bail!("No change found for session ID: {}", session_id);
                }
            }
        }
        Commands::Describe {
            session_id,
            message,
        } => {
            jjagent::describe_session_change(&session_id, &message)?;
        }
    }

    Ok(())
}
