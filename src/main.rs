#[macro_use]
extern crate log;
extern crate simplelog;

extern crate serde;

use anyhow::{Context, Result};
use chrono::prelude::*;
use clap_verbosity_flag::Verbosity;
use confy::ConfyError;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::{Deserialize, Serialize};
use simplelog::*;
use std::io::Write;
use std::path::PathBuf;
use structopt::clap::crate_version;
use structopt::StructOpt;
use text_io::read;

const APP_NAME: &'static str = "gtm-okr";

#[derive(Debug, StructOpt)]
#[structopt(name = "gtm-okr", version = crate_version!(), about = "Fetch GTMHub data", rename_all = "kebab-case")]
struct Cli {
    /// The config file to use
    #[structopt(short, long, parse(from_os_str))]
    pub config_file: Option<PathBuf>,

    /// Set verbosity default is just errors
    #[structopt(flatten)]
    verbose: Verbosity,

    #[structopt(subcommand)]
    cmd: Command,
}
#[derive(Debug, StructOpt)]
enum Command {
    /// Display teams
    Teams {
        /// Show team IDs too
        #[structopt(short, long)]
        ids: bool,
    },
    /// Display sessions
    Sessions {
        /// Fetch all not just active
        #[structopt(short, long, conflicts_with("current"))]
        all: bool,
        /// Fetch just current
        #[structopt(short, long, conflicts_with("all"))]
        current: bool,

        /// Show team IDs too
        #[structopt(short, long)]
        ids: bool,
    },
    /// Display goals
    Goals,
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct MyConfig {
    account_id: String,
    api_token: String,
}

#[tokio::main]
pub async fn main() {
    ::std::process::exit(match run().await {
        Ok(_) => 0,
        Err(err) => {
            error!("Error: {}", err);
            1
        }
    });
}

pub async fn run() -> Result<()> {
    // Allow ctrl-c to interupt and abort
    ctrlc::set_handler(move || {
        println!("received Ctrl+C!");
    })
    .with_context(|| format!("Could not set Ctrl-C handler"))?;

    // Check command line params
    let args = Cli::from_args();

    setup_logging(&args.verbose).expect("Failed to initialize logging");

    debug!("Got args {:?}", args);

    let cfg: MyConfig = get_config_from_file(&args.config_file)?;
    debug!("Got config {:?}", cfg);

    match args.cmd {
        Command::Teams { ids } => {
            let teams = get_teams(&cfg).await?;
            if ids {
                display_teams_and_ids(teams)
            } else {
                display_teams(teams)
            }
        }
        Command::Sessions { ids, all, current } => {
            let sessions = get_sessions(&cfg).await?;
            if all {
                display_sessions(sessions.items.into_iter(), ids);
            } else if current {
                let utc = Utc::now();
                let today = utc.to_rfc3339();
                display_sessions(
                    sessions
                        .items
                        .into_iter()
                        .filter(|s| &s.end >= &today && &s.start <= &today),
                    ids,
                );
            } else {
                display_sessions(
                    sessions.items.into_iter().filter(|s| s.status != "closed"),
                    ids,
                );
            };
        }
        Command::Goals => {
            let goals = get_goals(&cfg).await?;
            let utc = Utc::now();
            let today = utc.to_rfc3339();
            let mut filtered: Vec<Goal> = goals
                .items
                .into_iter()
                .filter(|g| {
                    &g.date_to >= &today
                        && &g.date_from <= &today
                        && g.assignee.assignee_type == "team"
                })
                .collect();
            filtered.sort_by(|a, b| {
                a.date_from
                    .cmp(&b.date_from)
                    .then_with(|| a.session_id.cmp(&b.session_id))
                    .then_with(|| a.assignee.name.cmp(&b.assignee.name))
            });
            let sessions = get_sessions(&cfg).await?;
            display_goals(filtered.into_iter(), &sessions.items);
        }
    }
    Ok(())
}

fn setup_logging(v: &Verbosity) -> Result<()> {
    Ok(TermLogger::init(
        match v.log_level().unwrap_or(log::Level::Error) {
            log::Level::Trace => LevelFilter::Trace,
            log::Level::Debug => LevelFilter::Debug,
            log::Level::Info => LevelFilter::Info,
            log::Level::Warn => LevelFilter::Warn,
            log::Level::Error => LevelFilter::Error,
        },
        // LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?)
}

fn get_config_from_file(config_file: &Option<PathBuf>) -> Result<MyConfig> {
    Ok(match config_file {
        Some(ref config_file) => confy::load_path(config_file),
        None => confy::load(APP_NAME),
    }
    .and_then(|cfg: MyConfig| {
        // If reading the config didn't throw an error, but produced a default
        // config with no values, then prompt the user.
        if cfg.api_token == "" {
            get_config_from_user(&config_file)
        } else {
            Ok(cfg)
        }
    })
    // And if some error happened on reading, try to prompt the user and write.
    .or_else(|_| get_config_from_user(&config_file))?)
}

fn get_config_from_user(
    config_file: &Option<PathBuf>,
) -> core::result::Result<MyConfig, ConfyError> {
    // No preference file found so prompt the user
    let mut stdo = std::io::stdout();
    print!("Enter the GTMHub account id: ");
    let _ = stdo.flush();
    let account_id: String = read!("{}\n");
    print!("Enter the GTMHub API token: ");
    let _ = stdo.flush();
    let api_token: String = read!("{}\n");
    let cfg = MyConfig {
        account_id,
        api_token,
    };
    // And then save to the preference file so we don't have to prompt again
    match config_file {
        Some(ref config_file) => confy::store_path(config_file, &cfg),
        None => confy::store(APP_NAME, &cfg),
    }
    .map(|_| cfg)
}

#[derive(Debug, Deserialize)]
struct TeamsResponse {
    items: Vec<Team>,
    #[serde(rename = "totalCount")]
    total_count: i64,
}
#[derive(Debug, Deserialize)]
struct Team {
    #[serde(rename = "accountId")]
    account_id: String,
    avatar: String,
    #[serde(rename = "dateCreated")]
    date_created: String,
    description: String,
    id: String,
    name: String,
    #[serde(rename = "parentId")]
    parent_id: String,
}

fn gtmclient(conf: &MyConfig, path: &str) -> reqwest::RequestBuilder {
    let client = reqwest::Client::new();
    client
        .get("https://app.us.gtmhub.com/api/v1".to_string() + path)
        .header(USER_AGENT, "User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_13_6) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/11.1.2 Safari/605.1.15")
        .header(ACCEPT, "application/json")
        .header("gtmhub-accountId", format!("{}", conf.account_id))
        .bearer_auth(&conf.api_token)
}

async fn get_teams(conf: &MyConfig) -> Result<TeamsResponse> {
    let response = gtmclient(&conf, "/teams").send().await?;

    debug!(
        "Full: {:?}\nGot status: {}\n",
        &response,
        &response.status()
    );
    let json: TeamsResponse = response.json().await?;
    info!("JSON: {:?}", &json);
    Ok(json)
}

fn display_teams(teams: TeamsResponse) {
    for t in teams.items.iter() {
        println!("* {}", t.name);
    }
}

fn display_teams_and_ids(teams: TeamsResponse) {
    for t in teams.items.iter() {
        println!("* {}: {}", t.id, t.name);
    }
}

#[derive(Debug, Deserialize)]
struct PlanningSessionsResponse {
    items: Vec<Session>,
    #[serde(rename = "totalCount")]
    total_count: i64,
}
#[derive(Debug, Deserialize)]
struct Session {
    #[serde(rename = "accountId")]
    account_id: String,
    end: String,
    id: String,
    #[serde(rename = "parentId")]
    parent_id: String,
    start: String,
    status: String,
    title: String,
}

async fn get_sessions(conf: &MyConfig) -> Result<PlanningSessionsResponse> {
    let request = gtmclient(&conf, "/sessions").query(&[
        ("fields", "id,accountId,end,parentId,start,status,title"),
        // ("filter", "{ status: {$ne: \"closed\" }}"),
        ("sort", "start"),
    ]);
    // .query(&[("filter", "{ status: {$eq: \"open\"} }"), ("sort", "start")]);
    debug!("Request: {:?}\n", &request);
    let response = request.send().await?;

    debug!(
        "Full: {:?}\nGot status: {}\n",
        &response,
        &response.status()
    );
    let json: PlanningSessionsResponse = response.json().await?;
    info!("JSON: {:?}", &json);
    Ok(json)
}

fn display_sessions<I>(sessions: I, ids: bool)
where
    I: IntoIterator<Item = Session>,
{
    for s in sessions {
        if ids {
            println!("* {}: {} ({})", s.id, s.title, s.status);
        } else {
            println!("* {} ({})", s.title, s.status);
        }
    }
}

#[derive(Debug, Deserialize)]
struct GoalsResponse {
    items: Vec<Goal>,
    #[serde(rename = "totalCount")]
    total_count: i64,
}

#[derive(Debug, Deserialize)]
struct Assignee {
    #[serde(rename = "accountId")]
    account_id: String,
    avatar: String,
    email: String,
    id: String,
    name: String,
    #[serde(rename = "type")]
    assignee_type: String, //AssigneeType, // "team" or "user"
}
#[derive(Debug, Deserialize)]
enum AssigneeType {
    #[serde(rename = "team")]
    Team,
    #[serde(rename = "user")]
    User,
}
#[derive(Debug, Deserialize)]
struct Confidence {
    date: String,
    reason: String,
    #[serde(rename = "userId")]
    user_id: String,
    value: f64,
}
#[derive(Debug, Deserialize)]
struct Metric {
    description: Option<String>,
    actual: f64,
    assignee: Option<Assignee>,
    critical: Option<f64>,
    confidence: Option<Confidence>,
    #[serde(rename = "dueDate")]
    due_date: Option<String>,
    /* #[serde(rename = "goalDescription")]
    goal_description: String,
    #[serde(rename = "goalId")]
    goal_id: String,
    #[serde(rename = "goalName")]
    goal_name: String,
    #[serde(rename = "goalOwnerId")]
    goal_owner_id: String, */
    #[serde(rename = "initialValue")]
    initial_value: Option<f64>,
    #[serde(rename = "manualType")]
    manual_type: Option<String>,
    name: String,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    target: f64,
    #[serde(rename = "targetOperator")]
    target_operator: Option<String>,
}
#[derive(Debug, Deserialize)]
struct Goal {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(rename = "aggregatedAttainment")]
    aggregated_attainment: Option<f64>,
    assignee: Assignee,
    attainment: f64,
    #[serde(rename = "attainmentTypeString")]
    attainment_type: Option<String>,
    #[serde(rename = "dateCreated")]
    date_created: String,
    #[serde(rename = "dateFrom")]
    date_from: String,
    #[serde(rename = "dateTo")]
    date_to: String,
    description: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: String,
    /* #[serde(rename = "fullAggregatedAttainment")]
    full_aggregated_attainment: f64, */
    id: String,
    #[serde(default)]
    metrics: Vec<Metric>,
    name: String,
    url: String,
}

async fn get_goals(conf: &MyConfig) -> Result<GoalsResponse> {
    let request = gtmclient(&conf, "/goals").query(&[
        ("fields", "accountId,sessionId,assignee,attainment,attainmentType,dateCreated,dateFrom,dateTo,description,fullAggregatedAttainment,id,metrics{id,confidence,name,attainment,description,actual,target},name,url"),
        ("sort", "-dateTo,sessionId,assignee.name"),
        // I'm extremely confused about why, but the filter stuff just doesn't work.
        // For that matter, neither does limit or skip.
        ("filter", r#"{"sessionId": "60e48512e442c8000146a86d"}"#),
    ]);
    debug!("Request: {:?}\n", &request);

    let response = request.send().await?;

    debug!(
        "Full: {:?}\nGot status: {}\n",
        &response,
        &response.status()
    );
    let json: GoalsResponse = response.json().await?;
    info!("JSON: {:?}", &json);
    Ok(json)
}

fn display_goals<I>(goals: I, sessions: &[Session])
where
    I: IntoIterator<Item = Goal>,
{
    let mut session_id = "".to_string();
    let mut team = "".to_string();
    for g in goals {
        if g.session_id != session_id {
            println!(
                "* **{}** ({} to {})",
                sessions
                    .iter()
                    .find(|s| s.id == g.session_id)
                    .map(|s| &s.title)
                    .unwrap_or(&g.session_id),
                g.date_from
                    .split_once('T')
                    .map(|x| x.0)
                    .unwrap_or(&g.date_from),
                g.date_to
                    .split_once('T')
                    .map(|x| x.0)
                    .unwrap_or(&g.date_from),
            );
            session_id = g.session_id.to_string();
            team = "".to_string();
        }
        if g.assignee.name != team {
            println!("    * **{}**", g.assignee.name);
            team = g.assignee.name.to_string();
        }
        println!("        * {} ({:.0}%)", g.name, g.attainment * 100.0);
        for m in g.metrics.iter() {
            println!("            * KR: {} ({}/{})", m.name, m.actual, m.target);
        }
    }
}

/*

/metrics/?goalids=
 */
