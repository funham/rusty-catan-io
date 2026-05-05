use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use catan_agents::{greedy::GreedyAgent, lazy::LazyAgent};
use catan_core::{
    agent::Agent,
    gameplay::game::{
        controller::{GameController, GameResult, GameRunStats, RunOptions},
        init::GameInitializationState,
    },
    math::dice::RandomDiceRoller,
};
use catan_runtime::config::{FieldConfig, MatchConfig, PlayerConfig};
use serde::Serialize;

#[derive(Debug)]
struct Args {
    config: PathBuf,
    games: u64,
    seed: u64,
    seed_stride: u64,
    max_turns: Option<u64>,
    json_summary: bool,
    no_log: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            config: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("data/configurations/greedy_brawl.json"),
            games: 1,
            seed: 0,
            seed_stride: 1,
            max_turns: None,
            json_summary: false,
            no_log: false,
        }
    }
}

#[derive(Debug)]
struct GameOutcome {
    result: GameResult,
    stats: GameRunStats,
}

#[derive(Debug, Default, Serialize)]
struct ResultCounts {
    wins: u64,
    limits: u64,
    interruptions: u64,
}

#[derive(Debug, Default, Serialize)]
struct Totals {
    turns_started: u64,
    turns_ended: u64,
    decision_requests: u64,
    regular_actions: u64,
    builds: u64,
    bank_trades: u64,
    dev_cards_bought: u64,
    dev_cards_used: u64,
    dice_rolls: u64,
    resources_distributed: u64,
    player_discards: u64,
    robber_moves: u64,
    action_rejections: u64,
}

impl Totals {
    fn add_stats(&mut self, stats: GameRunStats) {
        self.turns_started += stats.turns_started;
        self.turns_ended += stats.turns_ended;
        self.decision_requests += stats.decision_requests;
        self.regular_actions += stats.regular_actions;
        self.builds += stats.builds;
        self.bank_trades += stats.bank_trades;
        self.dev_cards_bought += stats.dev_cards_bought;
        self.dev_cards_used += stats.dev_cards_used;
        self.dice_rolls += stats.dice_rolls;
        self.resources_distributed += stats.resources_distributed;
        self.player_discards += stats.player_discards;
        self.robber_moves += stats.robber_moves;
        self.action_rejections += stats.action_rejections;
    }
}

#[derive(Debug, Serialize)]
struct Rates {
    simulations_per_sec: f64,
    turns_per_sec: f64,
    regular_actions_per_sec: f64,
    builds_per_sec: f64,
    decision_requests_per_sec: f64,
    legal_candidates_per_sec: Option<f64>,
}

#[derive(Debug, Serialize)]
struct Environment {
    git_commit: Option<String>,
    rustc: Option<String>,
    cargo: Option<String>,
    target_arch: &'static str,
    target_os: &'static str,
    os: Option<String>,
    profile: &'static str,
    bench_counters_enabled: bool,
}

#[derive(Debug, Serialize)]
struct Summary {
    config: String,
    games: u64,
    seed: u64,
    seed_stride: u64,
    max_turns: Option<u64>,
    elapsed_secs: f64,
    result_counts: ResultCounts,
    totals: Totals,
    rates: Rates,
    legal_counters: Option<LegalCounterSummary>,
    environment: Environment,
}

#[derive(Debug, Serialize)]
struct LegalCounterSummary {
    city_candidates: u64,
    settlement_candidates: u64,
    road_candidates: u64,
    dev_card_candidates: u64,
    roadbuild_candidates: u64,
    total_candidates: u64,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;
    if !args.no_log {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
    }

    let config = load_config(&args.config)?;
    validate_benchmark_config(&config)?;
    let effective_max_turns = args.max_turns.or(config.limits.max_turns);

    #[cfg(feature = "bench-counters")]
    catan_core::gameplay::game::legal::counters::reset();

    let started = Instant::now();
    let mut totals = Totals::default();
    let mut result_counts = ResultCounts::default();

    for game_index in 0..args.games {
        let seed = args
            .seed
            .wrapping_add(args.seed_stride.wrapping_mul(game_index));
        let outcome = run_one_game(&config, seed, args.max_turns)?;
        totals.add_stats(outcome.stats);
        match outcome.result {
            GameResult::Win(_) => result_counts.wins += 1,
            GameResult::LimitReached { .. } => result_counts.limits += 1,
            GameResult::Interrupted { .. } => result_counts.interruptions += 1,
        }
    }

    let elapsed = started.elapsed();
    let json_summary = args.json_summary;
    let summary = build_summary(args, effective_max_turns, elapsed, totals, result_counts);
    if summary.environment.bench_counters_enabled && summary.legal_counters.is_none() {
        return Err("bench counters were expected but not available".to_owned());
    }

    if summary.config.is_empty() {
        unreachable!("config path is always present");
    }

    if json_summary {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        print_human_summary(&summary);
    }

    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" => parsed.config = next_path(&mut iter, "--config")?,
            "--games" => parsed.games = next_parse(&mut iter, "--games")?,
            "--seed" => parsed.seed = next_parse(&mut iter, "--seed")?,
            "--seed-stride" => parsed.seed_stride = next_parse(&mut iter, "--seed-stride")?,
            "--max-turns" => parsed.max_turns = Some(next_parse(&mut iter, "--max-turns")?),
            "--json-summary" => parsed.json_summary = true,
            "--no-log" => parsed.no_log = true,
            "--help" | "-h" => return Err(help_text()),
            other => return Err(format!("unknown argument: {other}\n\n{}", help_text())),
        }
    }

    if parsed.games == 0 {
        return Err("--games must be greater than zero".to_owned());
    }

    Ok(parsed)
}

fn next_path(iter: &mut impl Iterator<Item = String>, name: &str) -> Result<PathBuf, String> {
    iter.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} requires a value"))
}

fn next_parse<T: std::str::FromStr>(
    iter: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<T, String> {
    iter.next()
        .ok_or_else(|| format!("{name} requires a value"))?
        .parse::<T>()
        .map_err(|_| format!("{name} has an invalid value"))
}

fn help_text() -> String {
    [
        "usage: catan-bench [--config PATH] [--games N] [--seed N]",
        "                   [--seed-stride N] [--max-turns N]",
        "                   [--json-summary] [--no-log]",
    ]
    .join("\n")
}

fn load_config(path: &PathBuf) -> Result<MatchConfig, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read config {}: {err}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {err}", path.display()))
}

fn validate_benchmark_config(config: &MatchConfig) -> Result<(), String> {
    if config.players.is_empty() {
        return Err("benchmark config must contain at least one player".to_owned());
    }
    if !config.observers.is_empty() {
        return Err("benchmark runner currently supports observer-free configs only".to_owned());
    }
    if config
        .players
        .iter()
        .any(|player| matches!(player, PlayerConfig::Cli | PlayerConfig::Random))
    {
        return Err(
            "benchmark runner currently supports only lazy and greedy in-process agents".to_owned(),
        );
    }
    Ok(())
}

fn run_one_game(
    config: &MatchConfig,
    seed: u64,
    max_turns_override: Option<u64>,
) -> Result<GameOutcome, String> {
    let mut agents = build_agents(&config.players);
    let init_state = build_initial_state(&config.field, seed);
    let state = GameController::init(init_state, &mut agents);
    let mut controller = GameController::new(state, agents);
    controller.use_seeded_randomness(seed);
    let mut dice = RandomDiceRoller::with_seed(seed);
    let result = controller.run_with_options(
        &mut dice,
        RunOptions {
            max_turns: max_turns_override.or(config.limits.max_turns),
            max_invalid_actions: config.limits.max_invalid_actions,
        },
    );

    Ok(GameOutcome {
        result,
        stats: controller.run_stats(),
    })
}

fn build_agents(players: &[PlayerConfig]) -> Vec<Box<dyn Agent>> {
    players
        .iter()
        .enumerate()
        .map(|(id, player)| match player {
            PlayerConfig::Lazy => Box::new(LazyAgent::new(id)) as Box<dyn Agent>,
            PlayerConfig::Greedy => Box::new(GreedyAgent::new(id)) as Box<dyn Agent>,
            PlayerConfig::Cli | PlayerConfig::Random => {
                unreachable!("unsupported agents are rejected during validation")
            }
        })
        .collect()
}

fn build_initial_state(config: &FieldConfig, seed: u64) -> GameInitializationState {
    match config {
        FieldConfig::Default => GameInitializationState::new_with_seed(Default::default(), seed),
    }
}

fn build_summary(
    args: Args,
    effective_max_turns: Option<u64>,
    elapsed: Duration,
    totals: Totals,
    result_counts: ResultCounts,
) -> Summary {
    let elapsed_secs = elapsed.as_secs_f64();
    let legal_counters = legal_counter_summary();
    let legal_total = legal_counters
        .as_ref()
        .map(|counters| counters.total_candidates as f64);

    Summary {
        config: args.config.display().to_string(),
        games: args.games,
        seed: args.seed,
        seed_stride: args.seed_stride,
        max_turns: effective_max_turns,
        elapsed_secs,
        rates: Rates {
            simulations_per_sec: rate(args.games, elapsed_secs),
            turns_per_sec: rate(totals.turns_started, elapsed_secs),
            regular_actions_per_sec: rate(totals.regular_actions, elapsed_secs),
            builds_per_sec: rate(totals.builds, elapsed_secs),
            decision_requests_per_sec: rate(totals.decision_requests, elapsed_secs),
            legal_candidates_per_sec: legal_total.map(|total| total / elapsed_secs),
        },
        totals,
        result_counts,
        legal_counters,
        environment: Environment {
            git_commit: command_output("git", &["rev-parse", "HEAD"]),
            rustc: command_output("rustc", &["-Vv"]),
            cargo: command_output("cargo", &["-V"]),
            target_arch: env::consts::ARCH,
            target_os: env::consts::OS,
            os: command_output("uname", &["-a"]).or_else(|| command_output("ver", &[])),
            profile: "release",
            bench_counters_enabled: cfg!(feature = "bench-counters"),
        },
    }
}

fn rate(count: u64, elapsed_secs: f64) -> f64 {
    if elapsed_secs == 0.0 {
        0.0
    } else {
        count as f64 / elapsed_secs
    }
}

fn legal_counter_summary() -> Option<LegalCounterSummary> {
    #[cfg(feature = "bench-counters")]
    {
        let counters = catan_core::gameplay::game::legal::counters::snapshot();
        let total_candidates = counters.city_candidates
            + counters.settlement_candidates
            + counters.road_candidates
            + counters.dev_card_candidates
            + counters.roadbuild_candidates;
        Some(LegalCounterSummary {
            city_candidates: counters.city_candidates,
            settlement_candidates: counters.settlement_candidates,
            road_candidates: counters.road_candidates,
            dev_card_candidates: counters.dev_card_candidates,
            roadbuild_candidates: counters.roadbuild_candidates,
            total_candidates,
        })
    }

    #[cfg(not(feature = "bench-counters"))]
    {
        None
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned()).filter(|text| !text.is_empty())
}

fn print_human_summary(summary: &Summary) {
    println!("config: {}", summary.config);
    println!(
        "games={} seed={} seed_stride={} elapsed={:.6}s",
        summary.games, summary.seed, summary.seed_stride, summary.elapsed_secs
    );
    println!(
        "results: wins={} limits={} interruptions={}",
        summary.result_counts.wins,
        summary.result_counts.limits,
        summary.result_counts.interruptions
    );
    println!(
        "totals: turns={} regular_actions={} decisions={} builds={} trades={} dice={}",
        summary.totals.turns_started,
        summary.totals.regular_actions,
        summary.totals.decision_requests,
        summary.totals.builds,
        summary.totals.bank_trades,
        summary.totals.dice_rolls
    );
    println!(
        "rates: simulations/s={:.3} turns/s={:.3} regular_actions/s={:.3} builds/s={:.3}",
        summary.rates.simulations_per_sec,
        summary.rates.turns_per_sec,
        summary.rates.regular_actions_per_sec,
        summary.rates.builds_per_sec
    );
    if let Some(counters) = &summary.legal_counters {
        println!(
            "legal_candidates: total={} city={} settlement={} road={} dev={} roadbuild={} rate/s={:.3}",
            counters.total_candidates,
            counters.city_candidates,
            counters.settlement_candidates,
            counters.road_candidates,
            counters.dev_card_candidates,
            counters.roadbuild_candidates,
            summary.rates.legal_candidates_per_sec.unwrap_or_default()
        );
    } else {
        println!("legal_candidates: disabled; rebuild with --features bench-counters");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_same_short_run_result_and_stats() {
        let config = load_config(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("data/configurations/greedy_brawl.json"),
        )
        .unwrap();

        let first = run_one_game(&config, 42, Some(5)).unwrap();
        let second = run_one_game(&config, 42, Some(5)).unwrap();

        assert_eq!(first.result, second.result);
        assert_eq!(first.stats, second.stats);
    }
}
