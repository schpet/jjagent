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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if env::var("JJAGENT_DISABLE").unwrap_or_default() == "1" {
        eprintln!("jjagent: Disabled via JJAGENT_DISABLE=1");
        return Ok(());
    }

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
                    let hook_name = match hook_cmd {
                        HookCommands::PreToolUse => "PreToolUse",
                        HookCommands::PostToolUse => "PostToolUse",
                        HookCommands::Stop => "Stop",
                    };
                    eprintln!("jjagent: {} hook called", hook_name);

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
                    };

                    // Output JSON response based on result
                    match result {
                        Ok(_) => {
                            let response = jjagent::hooks::HookResponse::continue_execution();
                            response.output();
                        }
                        Err(e) => {
                            let response = jjagent::hooks::HookResponse::stop(e.to_string());
                            response.output();
                            return Err(e);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
