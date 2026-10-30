use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use tracing_subscriber::EnvFilter;

use trlctl::init::{self, Prompt, Scope};
use trlctl::validate::{Finding, Severity, validate};

/// Exit status when a command fails: bad usage, unreadable files, refused write.
const EXIT_ERROR: u8 = 2;
/// Exit status of `validate` when the layout breaks the specification;
/// warnings alone leave the status at 0.
const EXIT_VIOLATIONS: u8 = 1;

/// Scaffold and validate repositories that follow the therealm Argo CD layout.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// `tracing` filter directive for diagnostics on stderr (e.g. `warn`, `trlctl=debug`)
    ///
    /// Syntax: <https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives>
    #[arg(long, env = "TRLCTL_LOG_FILTER", default_value = "warn", global = true)]
    log_filter: String,

    /// Format of the diagnostics on stderr
    #[arg(long, env = "TRLCTL_LOG_FORMAT", value_enum, default_value_t = LogFormat::Text, global = true)]
    log_format: LogFormat,
}

#[derive(Clone, Copy, ValueEnum)]
enum LogFormat {
    Text,
    Json,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a layout: `apps/`, `envs/` and `clusters/<cluster>/cluster.yaml`
    Init(InitArgs),
    /// Check a layout against the specification
    Validate(ValidateArgs),
}

#[derive(Args)]
struct InitArgs {
    /// Directory to initialise
    #[arg(long, env = "TRLCTL_DIR", default_value = ".")]
    dir: PathBuf,

    /// Name of the cluster to create under `clusters/`; prompted for when missing
    #[arg(long, env = "TRLCTL_CLUSTER")]
    cluster: Option<String>,

    /// URL of the repository holding the repo catalogue; defaults to the
    /// `origin` remote of the directory's Git repository, in `https://` form
    #[arg(long, env = "TRLCTL_CATALOG_URL")]
    catalog_url: Option<String>,

    /// URL of the repository holding the repo platform; defaults to the
    /// catalogue URL (with `--platform-only`, to the `origin` remote)
    #[arg(long, env = "TRLCTL_PLATFORM_URL")]
    platform_url: Option<String>,

    /// Revision both repositories are read at
    #[arg(long, env = "TRLCTL_REVISION", default_value = "main")]
    revision: String,

    /// Initialise the repo catalogue only (`apps/`)
    #[arg(long, env = "TRLCTL_CATALOG_ONLY", conflicts_with = "platform_only")]
    catalog_only: bool,

    /// Initialise the repo platform only (`envs/`, `clusters/`)
    #[arg(long, env = "TRLCTL_PLATFORM_ONLY")]
    platform_only: bool,

    /// Overwrite an existing `cluster.yaml`
    #[arg(long, env = "TRLCTL_FORCE")]
    force: bool,
}

#[derive(Args)]
struct ValidateArgs {
    /// Layout root of the repo catalogue (the directory holding `apps/`)
    #[arg(long, env = "TRLCTL_CATALOG", default_value = ".")]
    catalog: PathBuf,

    /// Layout root of the repo platform (the directory holding `envs/` and `clusters/`)
    #[arg(long, env = "TRLCTL_PLATFORM", default_value = ".")]
    platform: PathBuf,

    /// Format of the findings on stdout
    #[arg(long, env = "TRLCTL_OUTPUT", value_enum, default_value_t = Output::Human)]
    output: Output,
}

#[derive(Clone, Copy, ValueEnum)]
enum Output {
    Human,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Err(error) = init_logging(&cli) {
        eprintln!("error: {error:#}");
        return ExitCode::from(EXIT_ERROR);
    }
    let result = match cli.command {
        Command::Init(args) => run_init(args),
        Command::Validate(args) => run_validate(&args),
    };
    result.unwrap_or_else(|error| {
        eprintln!("error: {error:#}");
        ExitCode::from(EXIT_ERROR)
    })
}

fn init_logging(cli: &Cli) -> Result<()> {
    let fmt = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(&cli.log_filter)?)
        .with_writer(io::stderr);
    match cli.log_format {
        LogFormat::Text => fmt.init(),
        LogFormat::Json => fmt.json().init(),
    }
    Ok(())
}

fn run_init(args: InitArgs) -> Result<ExitCode> {
    let scope = match (args.catalog_only, args.platform_only) {
        (true, _) => Scope::CatalogOnly,
        (_, true) => Scope::PlatformOnly,
        _ => Scope::Full,
    };
    let options = init::Options {
        dir: args.dir,
        cluster: args.cluster,
        catalog_url: args.catalog_url,
        platform_url: args.platform_url,
        revision: args.revision,
        scope,
        force: args.force,
    };
    for file in init::run(&options, &mut TerminalPrompt)? {
        println!("{}", file.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn run_validate(args: &ValidateArgs) -> Result<ExitCode> {
    let findings = validate(&args.catalog, &args.platform)?;
    match args.output {
        Output::Human => print_human(&findings),
        Output::Json => println!("{}", serde_json::json!({ "findings": findings })),
    }
    Ok(if findings.iter().any(Finding::is_error) {
        ExitCode::from(EXIT_VIOLATIONS)
    } else {
        ExitCode::SUCCESS
    })
}

fn print_human(findings: &[Finding]) {
    for finding in findings {
        let severity = match finding.severity {
            Severity::Error => "",
            Severity::Warning => "warning: ",
        };
        match &finding.section {
            Some(section) => println!("{}: {severity}{section}: {}", finding.file, finding.message),
            None => println!("{}: {severity}{}", finding.file, finding.message),
        }
    }
    let errors = findings.iter().filter(|f| f.is_error()).count();
    let warnings = findings.len() - errors;
    let summary = match errors {
        0 => "the layout follows the specification".to_owned(),
        1 => "1 violation".to_owned(),
        count => format!("{count} violations"),
    };
    match warnings {
        0 => eprintln!("{summary}"),
        1 => eprintln!("{summary}, 1 warning"),
        count => eprintln!("{summary}, {count} warnings"),
    }
}

struct TerminalPrompt;

impl Prompt for TerminalPrompt {
    fn ask(&mut self, question: &str, missing_flag: &str) -> Result<String> {
        if !io::stdin().is_terminal() {
            bail!("{missing_flag} is required when stdin is not a terminal");
        }
        loop {
            eprint!("{question}: ");
            io::stderr().flush()?;
            let mut answer = String::new();
            if io::stdin().lock().read_line(&mut answer)? == 0 {
                bail!("{missing_flag} is required: no answer on stdin");
            }
            let answer = answer.trim();
            if !answer.is_empty() {
                return Ok(answer.to_owned());
            }
        }
    }
}
