mod cli;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Score(args) => run_process(args, false).await,
        Commands::Describe(args) => run_process(args, true).await,
        Commands::Init => run_init(),
    }
}

async fn run_process(args: cli::ProcessArgs, describe_only: bool) -> Result<()> {
    imgcull::setup_logging(args.verbose, args.quiet, args.log.as_deref())?;

    let config_dir = dirs::config_dir()
        .map(|d| d.join("imgcull"))
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory"))?;
    let mut config = imgcull::config::Config::load(&config_dir.join("config.toml"))?;

    // CLI overrides
    if let Some(c) = args.concurrency {
        config.default_settings.concurrency = c;
    }
    if let Some(ref p) = args.provider {
        config.default_settings.description_provider = p.clone();
        config.default_settings.scoring_provider = p.clone();
    }
    if let Some(ref p) = args.description_provider {
        config.default_settings.description_provider = p.clone();
    }
    if let Some(ref p) = args.scoring_provider {
        config.default_settings.scoring_provider = p.clone();
    }
    if let Some(ref dims) = args.dimensions {
        config.scoring.dimensions = dims.clone();
    }
    if args.backup {
        config.default_settings.backup = true;
    }

    let prompts_path = args
        .prompts
        .clone()
        .unwrap_or_else(|| config_dir.join("prompts.toml"));
    let prompts = imgcull::config::Prompts::load(&prompts_path)?;

    let images = imgcull::discovery::discover_images(&args.paths);
    if images.is_empty() {
        eprintln!("No supported images found.");
        return Ok(());
    }
    eprintln!("Found {} images to process.", images.len());

    // Dry-run: list images without requiring API keys or LLM clients
    if args.dry_run {
        for image in &images {
            let name = image.file_name().unwrap_or_default().to_string_lossy();
            eprintln!("  [dry-run] Would process: {name}");
        }
        return Ok(());
    }

    let clients = Arc::new(imgcull::llm::LlmClients::new(&config, &prompts)?);

    let options = imgcull::pipeline::PipelineOptions {
        no_description: args.no_description,
        no_rating: args.no_rating,
        backup: config.default_settings.backup,
        force: args.force,
        dry_run: args.dry_run,
        score_only: false,
        describe_only,
    };

    imgcull::pipeline::run_pipeline(images, &config, &prompts, clients, options).await
}

fn run_init() -> Result<()> {
    let config_dir = dirs::config_dir()
        .map(|d| d.join("imgcull"))
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory"))?;

    std::fs::create_dir_all(&config_dir)?;

    let config_path = config_dir.join("config.toml");
    if !config_path.exists() {
        let default = imgcull::config::Config::default();
        let toml_str = toml::to_string_pretty(&default)?;
        std::fs::write(&config_path, toml_str)?;
        eprintln!("Created {}", config_path.display());
    } else {
        eprintln!("Config already exists: {}", config_path.display());
    }

    let prompts_path = config_dir.join("prompts.toml");
    if !prompts_path.exists() {
        let default = imgcull::config::Prompts::default();
        let toml_str = toml::to_string_pretty(&default)?;
        std::fs::write(&prompts_path, toml_str)?;
        eprintln!("Created {}", prompts_path.display());
    } else {
        eprintln!("Prompts already exists: {}", prompts_path.display());
    }

    let env_example_path = config_dir.join(".env.example");
    if !env_example_path.exists() {
        std::fs::write(
            &env_example_path,
            "# imgcull — API keys for LLM providers\n\
             # Copy this file to .env in your photos directory and fill in the keys you need.\n\n\
             # ANTHROPIC_API_KEY=sk-ant-...\n\
             # OPENAI_API_KEY=sk-...\n\
             # GEMINI_API_KEY=...\n\
             # DEEPSEEK_API_KEY=...\n",
        )?;
        eprintln!("Created {}", env_example_path.display());
    }

    eprintln!("Done. Edit files in: {}", config_dir.display());
    Ok(())
}
